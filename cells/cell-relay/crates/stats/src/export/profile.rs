use super::Closed;
use crate::windows::{Error, Projection};
use agw_diff::Side;
use agw_inspector::{Number, Precision, Value};
use chrono::SecondsFormat;
use serde_json::json;
use sha2::{Digest, Sha256};

const BOUNDS: [i64; 15] = [
    -1_000_000_000_000,
    -1_000_000_000,
    -1_000_000,
    -1000,
    -100,
    -10,
    -1,
    0,
    1,
    10,
    100,
    1000,
    1_000_000,
    1_000_000_000,
    1_000_000_000_000,
];
fn invalid<T>(_: T) -> Error {
    Error::InvalidInput
}
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
/// Background only. Frozen v1 projection with exact token hashing and no f64 parse.
pub fn project(closed: &Closed, cohort: &str) -> Result<Vec<u8>, Error> {
    let s = &closed.scope;
    let segment = serde_json::to_string(&json!([
        "ti48",
        s.connection,
        s.generation,
        s.segment,
        s.window
    ]))
    .map_err(invalid)?;
    if [
        s.tenant.as_str(),
        s.vendor.as_str(),
        s.operation.as_str(),
        segment.as_str(),
        cohort,
    ]
    .iter()
    .any(|v| v.is_empty() || v.chars().count() > 256 || v.len() > 1024)
        || s.field.chars().count() > 1024
        || s.field.len() > 1024
    {
        return Err(Error::InvalidInput);
    }
    let mut types = [0_u64; 8];
    let mut values = vec![0_u64; 64];
    let mut hll = vec![0_u8; 64];
    let mut numeric = [0_u64; 16];
    let mut charset = [0_u64; 6];
    let mut excluded = 0_u64;
    let mut string_bytes = 0_usize;
    if closed.window.samples().is_empty() || closed.window.samples().len() > 4096 {
        return Err(Error::InvalidInput);
    }
    for sample in closed.window.samples() {
        let (index, token) = match sample.value() {
            None => (0, None),
            Some(Value::Null) => (1, None),
            Some(Value::Bool(b)) => (2, Some(format!("boolean:{b}"))),
            Some(Value::String(s)) => {
                string_bytes += s.len();
                if string_bytes > 65536 {
                    return Err(Error::ResourceLimit);
                }
                for c in s.chars() {
                    let category = if c.is_ascii_alphabetic() {
                        0
                    } else if c.is_ascii_digit() {
                        1
                    } else if matches!(
                        c,
                        ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
                            ..='\u{200a}'
                                | '\u{2028}'
                                | '\u{2029}'
                                | '\u{202f}'
                                | '\u{205f}'
                                | '\u{3000}'
                                | '\u{feff}'
                    ) {
                        2
                    } else if c.is_ascii_graphic() {
                        3
                    } else if c.is_ascii_control() {
                        4
                    } else {
                        5
                    };
                    charset[category] += 1;
                }
                (3, Some(format!("string:{s}")))
            }
            Some(Value::Number(n)) => {
                let lexeme = n.lexeme();
                let index = if lexeme.contains(['.', 'e', 'E']) {
                    5
                } else {
                    4
                };
                let exact =
                    if closed.window.projection == Projection::ExactInteger(Precision::Observed) {
                        match n {
                            Number::I64(v) if v.unsigned_abs() <= 9_007_199_254_740_991 => Some(*v),
                            Number::U64(v) if *v <= 9_007_199_254_740_991 => Some(*v as i64),
                            _ => None,
                        }
                    } else {
                        None
                    };
                if let Some(v) = exact {
                    numeric[BOUNDS.iter().position(|bound| v < *bound).unwrap_or(15)] += 1;
                } else {
                    excluded += 1;
                }
                (index, Some(format!("number:{lexeme}")))
            }
            _ => return Err(Error::InvalidInput),
        };
        types[index] += 1;
        if let Some(token) = token {
            let hash = Sha256::digest(token.as_bytes());
            let bucket = usize::from(hash[0] & 63);
            values[bucket] += 1;
            let mut rank = 1;
            for bit in 8..66 {
                if hash[bit / 8] & (1 << (7 - bit % 8)) != 0 {
                    break;
                }
                rank += 1;
            }
            hll[bucket] = hll[bucket].max(rank);
        }
    }
    let mut value = json!({"contract":"field_profile", "version":1,
        "scope":{"tenant":s.tenant,"vendor":s.vendor,"operation":s.operation,
            "side":if s.side == Side::Request {"request"} else {"response"},
            "tenant_segment":segment,"volume_cohort":cohort,"field":s.field},
        "start":closed.start.to_rfc3339_opts(SecondsFormat::Millis,true),
        "end":closed.end.to_rfc3339_opts(SecondsFormat::Millis,true),
        "types":types,"values":values,"hll":hll,"numeric":numeric,
        "excluded_numbers":excluded,"charset":charset});
    let times: Vec<_> = closed
        .window
        .samples()
        .iter()
        .map(|s| s.at().to_rfc3339())
        .collect();
    let identity = serde_json::to_vec(&json!([value, closed.id, times])).map_err(invalid)?;
    value["window_id"] = hex(&Sha256::digest(identity)).into();
    let bytes = serde_json::to_vec(&value).map_err(invalid)?;
    if bytes.len() > 65536 {
        return Err(Error::ResourceLimit);
    }
    // Generated type validates scope, shape, fixed arrays and version. Counters
    // are constructed above within the raw-window caps, not unchecked f64 bounds.
    let _: agw_audit::contracts::field_profile::FieldProfile =
        serde_json::from_slice(&bytes).map_err(invalid)?;
    Ok(bytes)
}
