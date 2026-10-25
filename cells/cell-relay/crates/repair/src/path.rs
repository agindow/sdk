//! Same escaping as TI-10/TI-11, with concrete array indices in audit paths.
pub(crate) fn escape(key: &str) -> String {
    if key.is_empty() {
        return "\\e".into();
    }
    key.chars()
        .flat_map(|c| {
            if matches!(c, '\\' | '.' | '[' | ']' | '$') {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}
pub(crate) fn child(parent: &str, key: &str) -> String {
    if parent == "$" {
        escape(key)
    } else {
        format!("{parent}.{}", escape(key))
    }
}
pub(crate) fn destination(path: &str, key: &str) -> Option<String> {
    if path == "$" {
        return None;
    }
    let mut escaped = false;
    let mut last_dot = None;
    for (i, c) in path.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '.' {
            last_dot = Some(i);
        }
    }
    Some(last_dot.map_or_else(
        || escape(key),
        |i| format!("{}.{}", &path[..i], escape(key)),
    ))
}
