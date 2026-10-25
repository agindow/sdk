//! Independent token oracle: skip JSON strings and collect number spellings.
use agw_inspector::Value;
use proptest::prelude::*;

pub fn lexemes(bytes: &[u8]) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' => i += 2,
                    b'"' => {
                        i += 1;
                        break;
                    }
                    _ => i += 1,
                }
            }
        } else if bytes[i].is_ascii_digit() || bytes[i] == b'-' {
            let start = i;
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_digit()
                    || matches!(bytes[i], b'.' | b'e' | b'E' | b'+' | b'-'))
            {
                i += 1;
            }
            tokens.push(String::from_utf8_lossy(&bytes[start..i]).into_owned());
        } else {
            i += 1;
        }
    }
    tokens
}

pub fn tree_lexemes(value: &Value) -> Vec<String> {
    match value {
        Value::Number(number) => vec![number.lexeme().into_owned()],
        Value::Array(values) => values.iter().flat_map(tree_lexemes).collect(),
        Value::Object(values) => values.iter().flat_map(|(_, v)| tree_lexemes(v)).collect(),
        _ => Vec::new(),
    }
}

pub fn number_token() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<i64>().prop_map(|n| n.to_string()),
        any::<u64>().prop_map(|n| n.to_string()),
        "-?[1-9][0-9]{19,60}",
        "-?(0|[1-9][0-9]{0,15})\\.[0-9]{1,30}([eE][+-]?[0-9]{1,5})?",
        "-?[1-9][0-9]{0,20}[eE][+-]?[0-9]{1,5}",
        Just("-0".into()),
    ]
}

/// Generate expected wire text, not an implementation value serialized through
/// the implementation under test. Duplicate keys and numeric-looking strings
/// ensure every input token (and only numeric tokens) is accounted for.
pub fn document() -> impl Strategy<Value = String> {
    let leaf = prop_oneof![
        number_token(),
        Just("null".into()),
        Just("true".into()),
        Just(r#""escaped \" -123 and \\ 9e99""#.into())
    ];
    leaf.prop_recursive(4, 64, 8, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..8)
                .prop_map(|v| format!("[{}]", v.join(","))),
            proptest::collection::vec(inner, 0..8).prop_map(|v| {
                let entries: Vec<_> = v.into_iter().map(|v| format!("\"key\":{v}")).collect();
                format!("{{ {} }}", entries.join(","))
            }),
        ]
    })
}
