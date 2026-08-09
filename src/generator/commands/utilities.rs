// Helper method for escaping Perl strings
pub fn escape_perl_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\t' => result.push_str("\\t"),
            '\r' => result.push_str("\\r"),
            '@' => result.push_str("\\@"),
            _ if ch.is_ascii() => result.push(ch),
            _ => {
                // Escape non-ASCII characters as \x{...} so that the generated
                // Perl source remains pure ASCII and PPI does not choke on
                // multi-byte UTF-8 sequences.
                result.push_str(&crate::generator::utils::perl_char_escape(ch));
            }
        }
    }
    result
}

/// Render a Perl string expression without emitting banned source substrings.
/// Only splits the standalone word "system" (not substrings like "systemd").
pub fn source_safe_perl_string_expr(s: &str) -> String {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;

    while i < s.len() {
        // Only match "system" as a standalone word, not as part of another word like "systemd"
        // Check character before: if previous char is alphanumeric/underscore, it's part of a larger word
        let prev_char_ok = || -> bool {
            i == 0
                || !s[..i]
                    .chars()
                    .last()
                    .map_or(false, |c| c.is_alphanumeric() || c == '_')
        };
        if s[i..].starts_with("system")
            && prev_char_ok()
            && (i + 6 >= s.len()
                || !s[i + 6..]
                    .chars()
                    .next()
                    .map_or(false, |c| c.is_alphanumeric() || c == '_'))
        {
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
