#!/usr/bin/env python3
"""Fix UTF-8 char boundary issues in parse_string_interpolation."""

with open('src/parser/words.rs', 'r') as f:
    content = f.read()

# Fix 1: else branch - advance by char width instead of byte
old1 = '''        } else {
            // Add to current literal
            current_literal.push(content[i..].chars().next().unwrap());
            i += 1;
        }'''

new1 = '''        } else {
            // Add to current literal
            let ch = content[i..].chars().next().unwrap();
            current_literal.push(ch);
            i += ch.len_utf8();
        }'''

count1 = content.count(old1)
print(f'Fix 1: found {count1} occurrences')
if count1 > 0:
    content = content.replace(old1, new1, 1)

# Fix 2: scanning loop for closing escaped backtick (pattern: \\\\\\`)
# The Rust source has: starts_with("\\\\`")
# In the file that's: starts_with("\\\\`")  where \\\\ in source = two backslashes in string
old2 = '''            // Find the closing escaped backtick
            i += 3; // skip the \\\\`
            let cmd_start = i;
            while i < content.len() && !content[i..].starts_with("\\\\`") {
                i += 1;
            }'''

new2 = '''            // Find the closing escaped backtick
            i += 3; // skip the \\\\`
            let cmd_start = i;
            while i < content.len() && !content[i..].starts_with("\\\\`") {
                let ch = content[i..].chars().next().unwrap_or('?');
                i += ch.len_utf8();
            }'''

count2 = content.count(old2)
print(f'Fix 2: found {count2} occurrences')
if count2 > 0:
    content = content.replace(old2, new2, 1)

# Fix 3: scanning loop for closing single-escaped backtick (pattern: \\`)
old3 = '''            // Find the closing escaped backtick
            i += 2; // skip the \\`
            let cmd_start = i;
            while i < content.len() && !content[i..].starts_with("\\`") {
                i += 1;
            }'''

new3 = '''            // Find the closing escaped backtick
            i += 2; // skip the \\`
            let cmd_start = i;
            while i < content.len() && !content[i..].starts_with("\\`") {
                let ch = content[i..].chars().next().unwrap_or('?');
                i += ch.len_utf8();
            }'''

count3 = content.count(old3)
print(f'Fix 3: found {count3} occurrences')
if count3 > 0:
    content = content.replace(old3, new3, 1)

# Fix 4: scanning loop for closing backtick `
old4 = '''            // Find the closing backtick
            i += 1; // skip the opening `
            let cmd_start = i;
            while i < content.len() && content[i..].chars().next() != Some('`') {
                i += 1;
            }'''

new4 = '''            // Find the closing backtick
            i += 1; // skip the opening `
            let cmd_start = i;
            while i < content.len() && content[i..].chars().next() != Some('`') {
                let ch = content[i..].chars().next().unwrap_or('?');
                i += ch.len_utf8();
            }'''

count4 = content.count(old4)
print(f'Fix 4: found {count4} occurrences')
if count4 > 0:
    content = content.replace(old4, new4, 1)

# Fix 5: $(...) scanning loop
old5 = '''            if i + 1 < content.len() && content[i + 1..].starts_with('(') {
                // Command substitution $(...)
                i += 2; // skip $ and (
                let cmd_start = i;
                let mut paren_count = 1;
                while i < content.len() && paren_count > 0 {
                    match content[i..].chars().next() {
                        Some('(') => paren_count += 1,
                        Some(')') => paren_count -= 1,
                        _ => {}
                    }
                    i += 1;
                }'''

new5 = '''            if i + 1 < content.len() && content[i + 1..].starts_with('(') {
                // Command substitution $(...)
                i += 2; // skip $ and (
                let cmd_start = i;
                let mut paren_count = 1;
                while i < content.len() && paren_count > 0 {
                    match content[i..].chars().next() {
                        Some('(') => paren_count += 1,
                        Some(')') => paren_count -= 1,
                        _ => {}
                    }
                    let ch = content[i..].chars().next().unwrap_or('?');
                    i += ch.len_utf8();
                }'''

count5 = content.count(old5)
print(f'Fix 5: found {count5} occurrences')
if count5 > 0:
    content = content.replace(old5, new5, 1)

# Fix 6: ${...} scanning loop
old6 = '''            } else if i + 1 < content.len() && content[i + 1..].starts_with('{') {
                // This is a parameter expansion ${...}
                i += 2; // skip $ and {
                let expansion_start = i;

                // Find the closing brace
                let mut brace_count = 1;
                while i < content.len() && brace_count > 0 {
                    match content[i..].chars().next() {
                        Some('{') => brace_count += 1,
                        Some('}') => brace_count -= 1,
                        _ => {}
                    }
                    i += 1;
                }'''

new6 = '''            } else if i + 1 < content.len() && content[i + 1..].starts_with('{') {
                // This is a parameter expansion ${...}
                i += 2; // skip $ and {
                let expansion_start = i;

                // Find the closing brace
                let mut brace_count = 1;
                while i < content.len() && brace_count > 0 {
                    match content[i..].chars().next() {
                        Some('{') => brace_count += 1,
                        Some('}') => brace_count -= 1,
                        _ => {}
                    }
                    let ch = content[i..].chars().next().unwrap_or('?');
                    i += ch.len_utf8();
                }'''

count6 = content.count(old6)
print(f'Fix 6: found {count6} occurrences')
if count6 > 0:
    content = content.replace(old6, new6, 1)

# Fix 7: variable name scanning loop
old7 = '''                    let var_start = i;
                    while i < content.len() {
                        let nc = content[i..].chars().next();
                        if let Some(c) = nc {
                            if c.is_alphanumeric() || c == '_' {
                                i += 1;
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }'''

new7 = '''                    let var_start = i;
                    while i < content.len() {
                        let nc = content[i..].chars().next();
                        if let Some(c) = nc {
                            if c.is_alphanumeric() || c == '_' {
                                let ch = content[i..].chars().next().unwrap_or('?');
                                i += ch.len_utf8();
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }'''

count7 = content.count(old7)
print(f'Fix 7: found {count7} occurrences')
if count7 > 0:
    content = content.replace(old7, new7, 1)

# Fix 8: There's a duplicate $ handler (the one at the end that handles literal $)
# Let me find and fix its scanning loops too
# The duplicate $ handler has similar patterns for $(...) and ${...}

with open('src/parser/words.rs', 'w') as f:
    f.write(content)

print('Done')
