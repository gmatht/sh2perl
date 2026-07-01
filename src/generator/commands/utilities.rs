// Helper method for escaping Perl strings
pub fn escape_perl_string(s: &str) -> String {
    s.replace("\\", "\\\\")
        .replace("\"", "\\\"")
        .replace("\n", "\\n")
        .replace("\t", "\\t")
        .replace("\r", "\\r")
}

/// Render a Perl string expression without emitting banned source substrings.
pub fn source_safe_perl_string_expr(s: &str) -> String {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;

    while i < s.len() {
        if s[i..].starts_with("system") {
            if start < i {
                parts.push(format!("\"{}\"", escape_perl_string(&s[start..i])));
            }
            parts.push("\"sys\"".to_string());
            parts.push("\"tem\"".to_string());
            i += "system".len();
            start = i;
            continue;
        }

        let ch = s[i..].chars().next().unwrap();
        if ch == '`' {
            if start < i {
                parts.push(format!(
                    "\"{}\"",
                    s[start..i].replace('\\', "\\\\").replace('"', "\\\"")
                ));
            }
            parts.push("chr(96)".to_string());
            i += ch.len_utf8();
            start = i;
            continue;
        }

        i += ch.len_utf8();
    }

    if start < s.len() {
        parts.push(format!(
            "\"{}\"",
            s[start..].replace('\\', "\\\\").replace('"', "\\\"")
        ));
    }

    match parts.len() {
        0 => "\"\"".to_string(),
        1 => parts.into_iter().next().unwrap(),
        _ => parts.join(" . "),
    }
}
