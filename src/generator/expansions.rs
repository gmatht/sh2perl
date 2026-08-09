use super::Generator;
use crate::ir::safe_perl_q_string;
use crate::ast::*;

/// Returns the Perl variable reference for a shell variable in a parameter expansion.
/// Uses `$ENV{var}` for variables not declared in the script (e.g. environment
/// variables like PWD, USER) to avoid `use strict` compilation errors, and
/// `${var}` for script-declared variables.
fn positional_param_ref(n: usize) -> String {
    format!("$_[{}]", n.saturating_sub(1))
}

pub(crate) fn parameter_var_scalar_ref(generator: &Generator, var_name: &str) -> String {
    // $0 is the script name, not a positional parameter — handle before numeric parse.
    if var_name == "0" {
        return "$0".to_string();
    }
    if let Ok(n) = var_name.parse::<usize>() {
        return positional_param_ref(n);
    }
    // If the variable is an indexed array, use element 0 (bash: ${array} = first element)
    if generator.indexed_arrays.contains(var_name) {
        return format!("${{{}}}[0]", var_name);
    }
    if generator.declared_locals.contains(var_name)
        || generator.function_level_vars.contains(var_name)
        || matches!(var_name, "#" | "*" | "-" | "?" | "$" | "!" | "0")
    {
        format!("${{{}}}", var_name)
    } else if var_name == "@" {
        // $@ in bash = all script arguments (top level) or function arguments
        // (inside a function).  In Perl, @ARGV / @_ is the corresponding array.
        // In scalar context, "@array" gives the space-separated string.
        if generator.fn_nesting_depth > 0 {
            format!("\"@_\"")
        } else {
            format!("\"@ARGV\"")
        }
    } else if var_name.contains('[')
        || var_name.contains(']')
        || var_name.contains('{')
        || var_name.contains('}')
    {
        // Variable name contains special characters (e.g. array[${index}]),
        // use a string literal fallback to avoid Perl syntax errors
        format!("$ENV{{'{}'}}", var_name.replace('\'', "\\'"))
    } else {
        // Undeclared variable — use $ENV{var} with // "" to avoid
        // "uninitialized value" warnings when the env var is not set.
        format!("($ENV{{{var}}} // q{{}})", var = var_name)
    }
}

/// Like `parameter_var_scalar_ref` but returns a bare reference that
/// can be assigned to (e.g. for `${var:=default}`).  Undeclared vars
/// are returned as `$ENV{var}` without the // "" wrapper because
/// `($ENV{var} // "")` is not assignable.
fn parameter_var_bare_assignable_ref(generator: &Generator, var_name: &str) -> String {
    // $0 is the script name, not a positional parameter.
    if var_name == "0" {
        return "$0".to_string();
    }
    if let Ok(n) = var_name.parse::<usize>() {
        return positional_param_ref(n);
    }
    // If the variable is an indexed array, use element 0 (bash: ${array} = first element)
    if generator.indexed_arrays.contains(var_name) {
        return format!("${{{}}}[0]", var_name);
    }
    if generator.declared_locals.contains(var_name)
        || generator.function_level_vars.contains(var_name)
        || matches!(var_name, "#" | "*" | "-" | "?" | "$" | "!" | "0")
    {
        format!("${}", var_name)
    } else if var_name == "@" {
        if generator.fn_nesting_depth > 0 {
            format!("\"@_\"")
        } else {
            format!("\"@ARGV\"")
        }
    } else {
        format!("$ENV{{{var}}}", var = var_name)
    }
}

/// Returns the bare sigil-prefixed Perl variable reference (e.g. `$var` or
/// `$ENV{var}`) for use in places like `$var =~ s/.../`.
pub(crate) fn parameter_var_bare_ref(generator: &Generator, var_name: &str) -> String {
    // $0 is the script name, not a positional parameter.
    if var_name == "0" {
        return "$0".to_string();
    }
    if let Ok(n) = var_name.parse::<usize>() {
        return positional_param_ref(n);
    }
    // If the variable is an indexed array, use element 0 (bash: ${array} = first element)
    if generator.indexed_arrays.contains(var_name) {
        return format!("${{{}}}[0]", var_name);
    }
    if generator.declared_locals.contains(var_name)
        || generator.function_level_vars.contains(var_name)
        || matches!(var_name, "#" | "*" | "-" | "?" | "$" | "!" | "0")
    {
        format!("${}", var_name)
    } else {
        // Undeclared variable — use $ENV{var} with // "" to avoid
        // "uninitialized value" warnings when the env var is not set.
        format!("($ENV{{{var}}} // q{{}})", var = var_name)
    }
}

pub fn generate_parameter_expansion_impl(
    generator: &mut Generator,
    pe: &ParameterExpansion,
) -> String {
    match &pe.operator {
        ParameterExpansionOperator::None => {
            // ${var} - just the variable
            // ${#var} - string length: ${#name} -> length($name)
            if pe.variable.starts_with('#')
                && !pe.variable.contains('[')
                && !pe.variable.contains(']')
            {
                let inner = &pe.variable[1..];
                let ref_str = if generator.declared_locals.contains(inner)
                    || generator.function_level_vars.contains(inner)
                {
                    format!("${}", inner)
                } else {
                    format!("$ENV{{{}}}", inner)
                };
                return format!("length({})", ref_str);
            }
            // ${var:offset} or ${var:offset:length} - substring
            if pe.variable.contains(':')
                && !pe.variable.contains('[')
                && !pe.variable.contains(']')
                && !pe.variable.starts_with(':')
            {
                if let Some(colon_pos) = pe.variable.find(':') {
                    let var_name = &pe.variable[..colon_pos];
                    let rest = &pe.variable[colon_pos + 1..];
                    let ref_str = if generator.declared_locals.contains(var_name)
                        || generator.function_level_vars.contains(var_name)
                    {
                        format!("${}", var_name)
                    } else {
                        format!("$ENV{{{}}}", var_name)
                    };
                    if let Some(second_colon) = rest.find(':') {
                        let offset = rest[..second_colon].trim();
                        let length = rest[second_colon + 1..].trim();
                        return format!("substr({}, {}, {})", ref_str, offset, length);
                    } else {
                        let offset = rest.trim();
                        return format!("substr({}, {})", ref_str, offset);
                    }
                }
            }
            // Check if this contains array access patterns like arr[1] or map[foo]
            if pe.variable.contains('[') && pe.variable.contains(']') {
                if let Some(bracket_start) = pe.variable.find('[') {
                    if let Some(bracket_end) = pe.variable.rfind(']') {
                        let var_name = &pe.variable[..bracket_start];
                        let key = &pe.variable[bracket_start + 1..bracket_end];

                        // An array name that was never assigned/declared is empty in
                        // bash (`${unset_arr[i]}` → q{}); emitting a bare `$arr[i]`
                        // would be a `use strict` compile error (undeclared @arr).
                        let known_array = generator.declared_locals.contains(var_name)
                            || generator.indexed_arrays.contains(var_name)
                            || generator.associative_arrays.contains(var_name)
                            || generator.function_level_vars.contains(var_name);

                        // Check if the key is numeric (indexed array) or string (associative array)
                        if !known_array {
                            "q{}".to_string()
                        } else if key.parse::<usize>().is_ok() {
                            // Indexed array access: arr[1] -> $arr[1]
                            format!("${}[{}]", var_name, key)
                        } else if generator.associative_arrays.contains(var_name) {
                            // Associative array access: map[foo] -> $map{'foo'}
                            // or map[$k] -> $map{$k}
                            if key.starts_with('$') {
                                // Variable key: map[$k] -> $map{$k}
                                // Use string concatenation to avoid complex format escapes
                                let mut result = String::from("$");
                                result.push_str(var_name);
                                result.push('{');
                                result.push_str(key);
                                result.push('}');
                                result
                            } else {
                                // Literal string key: map[foo] -> $map{'foo'}
                                let mut result = String::from("$");
                                result.push_str(var_name);
                                result.push_str("{'");
                                result.push_str(&key.replace("'", "\\'"));
                                result.push_str("'}");
                                result
                            }
                        } else {
                            // Indexed array access with variable/expression key: arr[i] -> $arr[$i]
                            format!(
                                "${}[{}]",
                                var_name,
                                generator.convert_arithmetic_to_perl(key)
                            )
                        }
                    } else {
                        parameter_var_scalar_ref(generator, &pe.variable)
                    }
                } else {
                    parameter_var_scalar_ref(generator, &pe.variable)
                }
            } else {
                parameter_var_scalar_ref(generator, &pe.variable)
            }
        }
        ParameterExpansionOperator::DefaultValue(default) => {
            // ${var:-default} - use default if var is empty
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            let default_expr = default_value_to_perl(generator, default);
            format!(
                "(defined {} && {} ne q{{}} ? {} : {})",
                r, r, r, default_expr
            )
        }
        ParameterExpansionOperator::AssignDefault(default) => {
            // ${var:=default} - assign default if var is empty
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            let assign_target = parameter_var_bare_assignable_ref(generator, &pe.variable);
            let default_expr = default_value_to_perl(generator, default);
            format!(
                "(defined {} && {} ne q{{}} ? {} : do {{ {} = {}; {} }})",
                r, r, r, assign_target, default_expr, r
            )
        }
        ParameterExpansionOperator::ErrorIfUnset(error) => {
            // ${var:?error} - error if var is empty/unset: print the error to
            // stderr and exit 1 (bash prints `bash: var: error` and exits 1).
            // NOTE: plain `die('...')` is NOT usable here — the harness runs
            // the generated file via `do $__f`, which catches the exception
            // and returns undef (exit 0), silently swallowing the failure.
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!(
                "(defined {} && {} ne q{{}} ? {} : do {{ print STDERR {}; exit 1; }})",
                r,
                r,
                r,
                safe_perl_q_string(&format!("{}: {}\n", pe.variable, error))
            )
        }
        ParameterExpansionOperator::BadSubstitution => {
            // `${arr[1]>2}` — bash rejects the expansion ("bad
            // substitution", skips the command); the Perl generator has
            // no skip mechanism, so render the raw variable reference
            // (best-effort; the corpus file is a bash-error probe).
            parameter_var_scalar_ref(generator, &pe.variable)
        }
        ParameterExpansionOperator::RemoveShortestSuffix(pattern) => {
            // ${var%suffix} - remove shortest suffix
            // To get shortest (rightmost) suffix, use the reverse trick:
            // reverse the var, apply shortest-prefix removal on the reversed pattern, then reverse
            let rev_pattern = reverse_glob_pattern(pattern);
            let regex = glob_to_perl_regex_nongreedy(&rev_pattern);
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!(
                "scalar reverse( (scalar reverse {}) =~ s/^{}//r )",
                r, regex
            )
        }
        ParameterExpansionOperator::RemoveLongestSuffix(pattern) => {
            // ${var%%suffix} - remove longest suffix (greedy from end)
            let regex = glob_to_perl_regex_greedy(pattern);
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!("{} =~ s/{}$//sr", r, regex)
        }
        ParameterExpansionOperator::RemoveShortestPrefix(pattern) => {
            // ${var#prefix} - remove shortest prefix (non-greedy from start)
            let regex = glob_to_perl_regex_nongreedy(pattern);
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!("{} =~ s/^{}//r", r, regex)
        }
        ParameterExpansionOperator::RemoveLongestPrefix(pattern) => {
            // ${var##prefix} - remove longest prefix (greedy from start)
            let regex = glob_to_perl_regex_greedy(pattern);
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!("{} =~ s/^{}//sr", r, regex)
        }
        ParameterExpansionOperator::SubstituteFirst(pattern, replacement) => {
            // ${var/pattern/replacement} - substitute first occurrence only
            let r = parameter_var_bare_ref(generator, &pe.variable);
            format!(
                "{} =~ s/{}/{}/rs",
                r,
                escape_regex_pattern(pattern),
                escape_regex_replacement(replacement)
            )
        }
        ParameterExpansionOperator::SubstituteAll(pattern, replacement) => {
            // ${var//pattern/replacement} - substitute all occurrences
            let r = parameter_var_bare_ref(generator, &pe.variable);
            format!(
                "{} =~ s/{}/{}/grs",
                r,
                escape_regex_pattern(pattern),
                escape_regex_replacement(replacement)
            )
        }
        ParameterExpansionOperator::UppercaseAll => {
            // ${var^^} - uppercase all characters
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!("uc({})", r)
        }
        ParameterExpansionOperator::LowercaseAll => {
            // ${var,,} - lowercase all characters
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!("lc({})", r)
        }
        ParameterExpansionOperator::UppercaseFirst => {
            // ${var^} - uppercase first character
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!("ucfirst({})", r)
        }
        ParameterExpansionOperator::Basename => {
            // ${var##*/} - get basename
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!("basename({})", r)
        }
        ParameterExpansionOperator::Dirname => {
            // ${var%/*} - get dirname
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!("dirname({})", r)
        }
        ParameterExpansionOperator::ArraySlice(offset, length) => {
            // Special case: ${#arr[@]} should be array length, not array slice
            if pe.variable.starts_with('#') && offset == "@" && length.is_none() {
                // ${#arr[@]} -> scalar(@arr) or scalar(keys %arr) for associative arrays
                let array_name = &pe.variable[1..]; // Remove the '#' prefix
                if generator.associative_arrays.contains(array_name) {
                    format!("scalar(keys %{})", array_name)
                } else {
                    format!("scalar(@{})", array_name)
                }
            } else if offset == "@" && length.is_none() {
                // ${map[@]} or ${!map[@]} - this represents array/map values or keys
                if pe.variable.starts_with('!') {
                    // ${!map[@]} -> keys %map (map keys iteration)
                    let map_name = &pe.variable[1..]; // Remove ! prefix
                    if map_name.contains(|c: char| !c.is_alphanumeric() && c != '_') {
                        // Variable name has special characters - this is likely an indirect
                        // expansion or prefix matching that can't be cleanly translated
                        "q{}".to_string()
                    } else {
                        format!("(keys %{})", map_name)
                    }
                } else {
                    // ${map[@]} -> @map (array iteration)
                    // For associative arrays, use values to match bash hash order.
                    // Perl hash order is randomized across processes but consistent within one run.
                    if generator.associative_arrays.contains(&pe.variable) {
                        format!("(values %{})", pe.variable)
                    } else {
                        format!("@{}", pe.variable)
                    }
                }
            } else {
                // Handle special argument list variables @ and *.
                // ${@:N} / ${*:N} means arguments starting at position N (1-indexed in bash).
                // In Perl, @ARGV / @_ is 0-indexed, so offset N-1.
                if pe.variable == "@" || pe.variable == "*" {
                    let array_ref = if generator.fn_nesting_depth > 0 {
                        "@_"
                    } else {
                        "@ARGV"
                    };
                    // Convert bash 1-indexed offset to Perl 0-indexed: N -> N-1
                    // If offset parses as a number, subtract 1; otherwise pass through.
                    let perl_offset = if let Ok(n) = offset.parse::<i64>() {
                        if n > 1 {
                            format!("{}", n - 1)
                        } else {
                            "0".to_string()
                        }
                    } else {
                        format!("({}) - 1", offset)
                    };
                    if let Some(length_str) = length {
                        format!(
                            "join(\" \", {}[{}..{}])",
                            array_ref, perl_offset, length_str
                        )
                    } else {
                        format!(
                            "join(\" \", {}[{}..$#{}])",
                            array_ref,
                            perl_offset,
                            if generator.fn_nesting_depth > 0 {
                                "_"
                            } else {
                                "ARGV"
                            }
                        )
                    }
                } else if pe.variable == "#" && offset == "@" && length.is_none() {
                    // ${#@} -> scalar(@ARGV) or scalar(@_)
                    if generator.fn_nesting_depth > 0 {
                        "scalar(@_)".to_string()
                    } else {
                        "scalar(@ARGV)".to_string()
                    }
                } else {
                    // Check if the variable is a scalar (not an array).
                    // If scalar, use substr(); otherwise use array slice.
                    let is_scalar = !generator.indexed_arrays.contains(&pe.variable)
                        && !generator.associative_arrays.contains(&pe.variable);
                    if is_scalar {
                        // Scalar substring: ${var:offset} or ${var:offset:length}
                        let var_ref = if generator.declared_locals.contains(&pe.variable)
                            || generator.function_level_vars.contains(&pe.variable)
                        {
                            format!("${}", pe.variable)
                        } else {
                            format!("$ENV{{{}}}", pe.variable)
                        };
                        if let Some(length_str) = length {
                            format!("substr({}, {}, {})", var_ref, offset, length_str)
                        } else {
                            format!("substr({}, {})", var_ref, offset)
                        }
                    } else {
                        // Use @main:: to reference the variable safely under strict mode
                        let var_ref = format!("@main::{}", pe.variable);
                        // ${var:offset:length} - array slice
                        if let Some(length_str) = length {
                            format!("join(\" \", {}[{}..{}])", var_ref, offset, length_str)
                        } else {
                            format!("join(\" \", {}[{}..])", var_ref, offset)
                        }
                    }
                }
            }
        }
    }
}

/// Render a parameter expansion for contexts that only hold `&Generator`
/// (the test-expression text path re-parses `${...}` operands).  Covers the
/// pattern-removal / substitution / case-mod operators, which need no
/// generator mutation.  Returns None for operators this helper doesn't cover
/// (callers fall back to their existing handling).
pub(crate) fn render_pattern_param_expansion(
    generator: &Generator,
    pe: &ParameterExpansion,
) -> Option<String> {
    let out = match &pe.operator {
        ParameterExpansionOperator::RemoveShortestSuffix(pattern) => {
            // ${var%suffix} — shortest (rightmost) suffix: reverse, strip
            // the shortest prefix of the reversed pattern, reverse back.
            let rev_pattern = reverse_glob_pattern(pattern);
            let regex = glob_to_perl_regex_nongreedy(&rev_pattern);
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!(
                "scalar reverse( (scalar reverse {}) =~ s/^{}//r )",
                r, regex
            )
        }
        ParameterExpansionOperator::RemoveLongestSuffix(pattern) => {
            let regex = glob_to_perl_regex_greedy(pattern);
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!("{} =~ s/{}$//sr", r, regex)
        }
        ParameterExpansionOperator::RemoveShortestPrefix(pattern) => {
            let regex = glob_to_perl_regex_nongreedy(pattern);
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!("{} =~ s/^{}//r", r, regex)
        }
        ParameterExpansionOperator::RemoveLongestPrefix(pattern) => {
            let regex = glob_to_perl_regex_greedy(pattern);
            let r = parameter_var_scalar_ref(generator, &pe.variable);
            format!("{} =~ s/^{}//sr", r, regex)
        }
        ParameterExpansionOperator::SubstituteFirst(pattern, replacement) => {
            let r = parameter_var_bare_ref(generator, &pe.variable);
            format!(
                "{} =~ s/{}/{}/rs",
                r,
                escape_regex_pattern(pattern),
                escape_regex_replacement(replacement)
            )
        }
        ParameterExpansionOperator::SubstituteAll(pattern, replacement) => {
            let r = parameter_var_bare_ref(generator, &pe.variable);
            format!(
                "{} =~ s/{}/{}/grs",
                r,
                escape_regex_pattern(pattern),
                escape_regex_replacement(replacement)
            )
        }
        ParameterExpansionOperator::UppercaseAll => {
            format!("uc({})", parameter_var_scalar_ref(generator, &pe.variable))
        }
        ParameterExpansionOperator::LowercaseAll => {
            format!("lc({})", parameter_var_scalar_ref(generator, &pe.variable))
        }
        ParameterExpansionOperator::UppercaseFirst => {
            format!("ucfirst({})", parameter_var_scalar_ref(generator, &pe.variable))
        }
        ParameterExpansionOperator::Basename => {
            format!("basename({})", parameter_var_scalar_ref(generator, &pe.variable))
        }
        ParameterExpansionOperator::Dirname => {
            format!("dirname({})", parameter_var_scalar_ref(generator, &pe.variable))
        }
        _ => return None,
    };
    // Wrap in parens: these operators bind looser than Perl comparison
    // operators (`a =~ s///r > b` parses as `(a =~ s///r) > b` only if
    // parenthesized — raw `=~` after `>` would apply to the comparison
    // result).  The string path emits them bare inside concatenations
    // where the surrounding quotes disambiguate; the test-expression
    // path needs explicit grouping.
    Some(format!("({})", out))
}

/// Convert a parameter expansion default value to Perl code.
/// If the default contains command substitutions (`$(...)` or backtick), parse and
/// convert them; otherwise emit a string literal.
pub(crate) fn default_value_to_perl(generator: &mut Generator, default: &str) -> String {
    // Check for nested ${...} parameter expansion
    if default.starts_with("${") && default.ends_with('}') {
        let inner = &default[2..default.len() - 1];
        if let Ok(pe) = crate::parser::words::parse_parameter_expansion_content(inner) {
            let perl = generator.generate_parameter_expansion(&pe);
            return perl;
        }
    }
    // Check for $(...) command substitution
    if default.starts_with("$(") && default.ends_with(')') {
        let inner = &default[2..default.len() - 1];
        if let Ok(command) = crate::parser::commands::parse_pipeline_from_text(inner) {
            let perl = generator.word_to_perl(&Word::CommandSubstitution(Box::new(command), None));
            return format!("do {{ my $_result = {}; $_result; }}", perl);
        }
    }
    // Check for backtick command substitution
    if default.starts_with('`') && default.ends_with('`') {
        let inner = &default[1..default.len() - 1];
        if let Ok(command) = crate::parser::commands::parse_pipeline_from_text(inner) {
            let perl = generator.word_to_perl(&Word::CommandSubstitution(Box::new(command), None));
            return format!("do {{ my $_result = {}; $_result; }}", perl);
        }
    }
    // Handle double-quoted string default: "" → '' (empty string), "text" → 'text'
    if default.starts_with('"') && default.ends_with('"') && default.len() >= 2 {
        let inner = &default[1..default.len() - 1];
        return format!("'{}'", inner.replace('\'', "'\\''"));
    }
    // Handle single-quoted string default: '' → '' (empty string), 'text' → 'text'
    if default.starts_with('\'') && default.ends_with('\'') && default.len() >= 2 {
        let inner = &default[1..default.len() - 1];
        return format!("'{}'", inner.replace('\'', "'\\''"));
    }
    // Fall back to string literal
    format!("'{}'", default)
}

// Helper methods for regex escaping
/// Convert a shell glob pattern to a Perl regex with non-greedy `*` (for shortest match)
pub(crate) fn glob_to_perl_regex_nongreedy(pattern: &str) -> String {
    let mut result = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => result.push_str(".*?"),
            '?' => result.push('.'),
            '[' => {
                // Pass character classes through, but escape '/' because the
                // caller embeds the result in s/// with '/' as the delimiter.
                result.push('[');
                while let Some(&c2) = chars.peek() {
                    chars.next();
                    if c2 == '/' {
                        result.push('\\');
                    }
                    result.push(c2);
                    if c2 == ']' {
                        break;
                    }
                }
            }
            '\\' => {
                if let Some(&next) = chars.peek() {
                    chars.next();
                    result.push('\\');
                    result.push(next);
                }
            }
            // Escape Perl regex metacharacters that aren't glob metacharacters.
            // Also escape '/' because the caller embeds the result in s/// with
            // '/' as the delimiter; unescaped '/' would break the substitution.
            '.' | '+' | '^' | '$' | '(' | ')' | '{' | '}' | '|' | '/' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

/// Convert a shell glob pattern to a Perl regex with greedy `*` (for longest match)
pub(crate) fn glob_to_perl_regex_greedy(pattern: &str) -> String {
    let mut result = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => result.push_str(".*"),
            '?' => result.push('.'),
            '[' => {
                result.push('[');
                while let Some(&c2) = chars.peek() {
                    chars.next();
                    if c2 == '/' {
                        result.push('\\');
                    }
                    result.push(c2);
                    if c2 == ']' {
                        break;
                    }
                }
            }
            '\\' => {
                if let Some(&next) = chars.peek() {
                    chars.next();
                    result.push('\\');
                    result.push(next);
                }
            }
            // Escape Perl regex metacharacters that aren't glob metacharacters.
            // Also escape '/' because the caller embeds the result in s/// with
            // '/' as the delimiter; unescaped '/' would break the substitution.
            '.' | '+' | '^' | '$' | '(' | ')' | '{' | '}' | '|' | '/' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

/// Reverse a glob pattern for use with the suffix reverse trick.
/// e.g. "o*" becomes "*o", "*abc" becomes "abc*"
pub(crate) fn reverse_glob_pattern(pattern: &str) -> String {
    // Collect glob tokens (literals, *, ?)
    let mut tokens: Vec<String> = Vec::new();
    let mut literal = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '?' => {
                if !literal.is_empty() {
                    tokens.push(literal.chars().rev().collect());
                    literal = String::new();
                }
                tokens.push(c.to_string());
            }
            '[' => {
                if !literal.is_empty() {
                    tokens.push(literal.chars().rev().collect());
                    literal = String::new();
                }
                let mut class = String::from("[");
                while let Some(&c2) = chars.peek() {
                    chars.next();
                    class.push(c2);
                    if c2 == ']' {
                        break;
                    }
                }
                tokens.push(class);
            }
            _ => literal.push(c),
        }
    }
    if !literal.is_empty() {
        tokens.push(literal.chars().rev().collect());
    }
    tokens.reverse();
    tokens.join("")
}

pub(crate) fn escape_regex_pattern(pattern: &str) -> String {
    // Escape special regex characters in the pattern.
    // Also escape '/' because the caller embeds the result in s/// with
    // '/' as the delimiter; unescaped '/' would break the substitution.
    pattern
        .replace("\\", "\\\\")
        .replace(".", "\\.")
        .replace("+", "\\+")
        .replace("*", "\\*")
        .replace("?", "\\?")
        .replace("^", "\\^")
        .replace("$", "\\$")
        .replace("/", "\\/")
        .replace("[", "\\[")
        .replace("]", "\\]")
        .replace("(", "\\(")
        .replace(")", "\\)")
        .replace("|", "\\|")
}

pub(crate) fn escape_regex_replacement(replacement: &str) -> String {
    // Escape special regex characters in the replacement string
    replacement
        .replace("\\", "\\\\")
        .replace("$", "\\$")
        .replace("&", "\\&")
        .replace("`", "\\`")
        .replace("'", "\\'")
}
