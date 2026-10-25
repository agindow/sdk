use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CharacterClass {
    AsciiLetter,
    AsciiDigit,
    Whitespace,
    Punctuation,
    Control,
    NonAscii,
    Replacement,
}
impl CharacterClass {
    pub(crate) const ALL: [Self; 7] = [
        Self::AsciiLetter,
        Self::AsciiDigit,
        Self::Whitespace,
        Self::Punctuation,
        Self::Control,
        Self::NonAscii,
        Self::Replacement,
    ];
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    IsoDate,
    IsoTimestamp,
    MonthDay,
    Digits,
    Unknown,
}
impl Format {
    pub(crate) const KNOWN: [Self; 4] = [
        Self::IsoDate,
        Self::IsoTimestamp,
        Self::MonthDay,
        Self::Digits,
    ];
    /// Anchored lexical shapes; these do not assert calendar validity.
    pub const fn pattern(self) -> Option<&'static str> {
        match self {
            Self::IsoDate => Some(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"),
            Self::IsoTimestamp => Some(
                r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?(Z|[+-][0-9]{2}:[0-9]{2})$",
            ),
            Self::MonthDay => {
                Some(r"^(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec) [0-9]{1,2}$")
            }
            Self::Digits => Some(r"^[0-9]+$"),
            Self::Unknown => None,
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fingerprint {
    classes: [usize; 7],
    formats: [usize; 5],
}
impl Fingerprint {
    pub fn classes(&self) -> BTreeSet<CharacterClass> {
        CharacterClass::ALL
            .into_iter()
            .filter(|c| self.class_count(*c) > 0)
            .collect()
    }
    pub fn class_count(&self, class: CharacterClass) -> usize {
        self.classes[class as usize]
    }
    pub fn format_count(&self, format: Format) -> usize {
        self.formats[format as usize]
    }
    pub(super) fn observe(&mut self, text: &str) {
        let mut present = [false; 7];
        for c in text.chars() {
            let class = if c == '\u{fffd}' {
                CharacterClass::Replacement
            } else if c.is_control() {
                CharacterClass::Control
            } else if c.is_ascii_alphabetic() {
                CharacterClass::AsciiLetter
            } else if c.is_ascii_digit() {
                CharacterClass::AsciiDigit
            } else if c.is_ascii_whitespace() {
                CharacterClass::Whitespace
            } else if c.is_ascii() {
                CharacterClass::Punctuation
            } else {
                CharacterClass::NonAscii
            };
            present[class as usize] = true;
        }
        for (count, present) in self.classes.iter_mut().zip(present) {
            *count += usize::from(present);
        }
        self.formats[recognize(text.as_bytes()) as usize] += 1;
    }
}
fn digits(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
}
fn date(bytes: &[u8]) -> bool {
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && digits(&bytes[..4])
        && digits(&bytes[5..7])
        && digits(&bytes[8..])
}
fn timestamp(bytes: &[u8]) -> bool {
    if bytes.len() < 20
        || !date(&bytes[..10])
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || !digits(&bytes[11..13])
        || !digits(&bytes[14..16])
        || !digits(&bytes[17..19])
    {
        return false;
    }
    let mut tail = &bytes[19..];
    if tail.first() == Some(&b'.') {
        let count = tail[1..].iter().take_while(|b| b.is_ascii_digit()).count();
        if count == 0 {
            return false;
        }
        tail = &tail[1 + count..];
    }
    tail == b"Z"
        || (tail.len() == 6
            && matches!(tail[0], b'+' | b'-')
            && tail[3] == b':'
            && digits(&tail[1..3])
            && digits(&tail[4..]))
}
fn recognize(bytes: &[u8]) -> Format {
    if date(bytes) {
        Format::IsoDate
    } else if timestamp(bytes) {
        Format::IsoTimestamp
    } else if (bytes.len() == 5 || bytes.len() == 6)
        && bytes[3] == b' '
        && [
            b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov",
            b"Dec",
        ]
        .iter()
        .any(|month| month.as_slice() == &bytes[..3])
        && digits(&bytes[4..])
    {
        Format::MonthDay
    } else if digits(bytes) {
        Format::Digits
    } else {
        Format::Unknown
    }
}
