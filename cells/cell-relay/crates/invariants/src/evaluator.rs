use crate::{
    Error, Evidence, Expected, Form, MAX_BINDINGS, MAX_ENUM, MAX_RULES, MAX_TERMS, MAX_TEXT,
    Relation, Rule, Scalar, Verdict, valid_text,
};

#[derive(Debug, Clone, Copy)]
enum EvidenceKind {
    Range,
    Vocabulary,
    Relation(Relation),
    Sum,
    Approximate,
}

type Check = fn(&CompiledRule, &[Scalar<'_>]) -> Verdict;

/// Prepared operands and a specialized check; no AST is retained or interpreted.
#[derive(Debug, Clone)]
pub struct CompiledRule {
    id: String,
    slots: Vec<usize>,
    numbers: [i128; 2],
    vocabulary: Vec<String>,
    check: Check,
    evidence_kind: EvidenceKind,
}
impl CompiledRule {
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Pure, bounded and allocation-free, including unavailable input paths.
    pub fn evaluate(&self, slots: &[Scalar<'_>]) -> Verdict {
        (self.check)(self, slots)
    }
    /// Exact computed evidence; unlike formatting, this path allocates nothing.
    pub fn evidence<'a>(&'a self, values: &[Scalar<'a>]) -> Option<Evidence<'a>> {
        let actual = *values.get(*self.slots.first()?)?;
        let expected = match self.evidence_kind {
            EvidenceKind::Vocabulary => {
                if !matches!(actual, Scalar::Text(v) if v.len() <= MAX_TEXT) {
                    return None;
                }
                Expected::Vocabulary(&self.vocabulary)
            }
            EvidenceKind::Range => {
                self.number(values, 0)?;
                Expected::Range {
                    min: self.numbers[0],
                    max: self.numbers[1],
                }
            }
            kind => {
                self.number(values, 0)?;
                let (value, relation, tolerance) = match kind {
                    EvidenceKind::Relation(relation) => (self.number(values, 1)?, relation, 0),
                    EvidenceKind::Sum => {
                        let mut value = 0_i128;
                        for i in 1..self.slots.len() {
                            value += self.number(values, i)?;
                        }
                        (value, Relation::Equal, self.numbers[0] as u64)
                    }
                    _ => (
                        self.number(values, 1)?,
                        Relation::Equal,
                        self.numbers[0] as u64,
                    ),
                };
                Expected::Numeric {
                    value,
                    relation,
                    tolerance,
                }
            }
        };
        Some(Evidence { actual, expected })
    }
    fn number(&self, values: &[Scalar<'_>], operand: usize) -> Option<i128> {
        match values.get(*self.slots.get(operand)?)? {
            Scalar::Integer(value) => Some(i128::from(*value)),
            _ => None,
        }
    }
}
#[derive(Debug, Clone)]
pub struct Program {
    rules: Vec<CompiledRule>,
}
impl Program {
    pub fn rules(&self) -> &[CompiledRule] {
        &self.rules
    }
}

/// Resolve names once. Duplicate bindings and IDs are rejected rather than
/// silently selecting ambiguous evidence. No truncation of oversized input.
pub fn compile(rules: &[Rule], bindings: &[&str]) -> Result<Program, Error> {
    if rules.len() > MAX_RULES || bindings.len() > MAX_BINDINGS {
        return Err(Error::Capacity);
    }
    for (i, name) in bindings.iter().enumerate() {
        if !valid_text(name) || bindings[..i].contains(name) {
            return Err(Error::Invalid);
        }
    }
    let mut output = Vec::with_capacity(rules.len());
    for (i, rule) in rules.iter().enumerate() {
        if !valid_text(&rule.id) || rules[..i].iter().any(|r| r.id == rule.id) {
            return Err(Error::Invalid);
        }
        output.push(compile_rule(rule, bindings)?);
    }
    Ok(Program { rules: output })
}

fn compile_rule(rule: &Rule, bindings: &[&str]) -> Result<CompiledRule, Error> {
    let slot = |name: &str| {
        bindings
            .iter()
            .position(|b| *b == name)
            .ok_or(Error::Invalid)
    };
    let mut result = CompiledRule {
        id: rule.id.clone(),
        slots: Vec::new(),
        numbers: [0; 2],
        vocabulary: Vec::new(),
        check: range,
        evidence_kind: EvidenceKind::Range,
    };
    match &rule.form {
        Form::Range { field, min, max } => {
            if min > max {
                return Err(Error::Invalid);
            }
            result.slots.push(slot(field)?);
            result.numbers = [i128::from(*min), i128::from(*max)];
        }
        Form::Enum { field, values } => {
            if values.is_empty() || values.len() > MAX_ENUM {
                return Err(Error::Capacity);
            }
            for (i, value) in values.iter().enumerate() {
                if value.len() > MAX_TEXT || values[..i].contains(value) {
                    return Err(Error::Invalid);
                }
            }
            result.slots.push(slot(field)?);
            result.vocabulary = values.clone();
            result.vocabulary.sort();
            result.check = enumeration;
            result.evidence_kind = EvidenceKind::Vocabulary;
        }
        Form::Relation {
            left,
            relation,
            right,
        } => {
            result.slots = vec![slot(left)?, slot(right)?];
            result.evidence_kind = EvidenceKind::Relation(*relation);
            result.check = match relation {
                Relation::Equal => equal,
                Relation::Less => less,
                Relation::LessEqual => less_equal,
                Relation::Greater => greater,
                Relation::GreaterEqual => greater_equal,
            };
        }
        Form::Ordering { earlier, later } => {
            result.slots = vec![slot(earlier)?, slot(later)?];
            result.check = less_equal;
            result.evidence_kind = EvidenceKind::Relation(Relation::LessEqual);
        }
        Form::Sum {
            total,
            terms,
            tolerance,
        } => {
            if terms.is_empty() || terms.len() > MAX_TERMS {
                return Err(Error::Capacity);
            }
            result.slots.push(slot(total)?);
            for term in terms {
                result.slots.push(slot(term)?);
            }
            result.numbers[0] = i128::from(*tolerance);
            result.check = sum;
            result.evidence_kind = EvidenceKind::Sum;
        }
        Form::CrossEndpoint {
            left,
            right,
            tolerance,
        } => {
            result.slots = vec![slot(left)?, slot(right)?];
            result.numbers[0] = i128::from(*tolerance);
            result.check = approximate;
            result.evidence_kind = EvidenceKind::Approximate;
        }
    }
    Ok(result)
}
fn verdict(value: bool) -> Verdict {
    if value {
        Verdict::Pass
    } else {
        Verdict::Violation
    }
}
fn range(rule: &CompiledRule, values: &[Scalar<'_>]) -> Verdict {
    let Some(n) = rule.number(values, 0) else {
        return Verdict::Unavailable;
    };
    verdict(n >= rule.numbers[0] && n <= rule.numbers[1])
}
fn enumeration(rule: &CompiledRule, values: &[Scalar<'_>]) -> Verdict {
    let Some(Scalar::Text(value)) = values.get(rule.slots[0]) else {
        return Verdict::Unavailable;
    };
    if value.len() > MAX_TEXT {
        return Verdict::Unavailable;
    }
    if rule
        .vocabulary
        .binary_search_by(|known| known.as_str().cmp(value))
        .is_ok()
    {
        Verdict::Pass
    } else {
        Verdict::EnumAddition
    }
}
macro_rules! comparison {
    ($name:ident, $operator:tt) => {
        fn $name(rule: &CompiledRule, values: &[Scalar<'_>]) -> Verdict {
            match (rule.number(values, 0), rule.number(values, 1)) {
                (Some(a), Some(b)) => verdict(a $operator b), _ => Verdict::Unavailable,
            }
        }
    }
}
comparison!(equal, ==);
comparison!(less, <);
comparison!(less_equal, <=);
comparison!(greater, >);
comparison!(greater_equal, >=);
fn approximate(rule: &CompiledRule, values: &[Scalar<'_>]) -> Verdict {
    match (rule.number(values, 0), rule.number(values, 1)) {
        (Some(a), Some(b)) => verdict((a - b).abs() <= rule.numbers[0]),
        _ => Verdict::Unavailable,
    }
}
fn sum(rule: &CompiledRule, values: &[Scalar<'_>]) -> Verdict {
    let Some(total) = rule.number(values, 0) else {
        return Verdict::Unavailable;
    };
    let mut expected = 0_i128;
    for i in 1..rule.slots.len() {
        let Some(value) = rule.number(values, i) else {
            return Verdict::Unavailable;
        };
        // At most MAX_TERMS i64 inputs: sum and difference fit in i128.
        expected += value;
    }
    verdict((total - expected).abs() <= rule.numbers[0])
}
