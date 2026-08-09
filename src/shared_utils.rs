// Removed unused import: use crate::ast::*;

use std::fs;
use std::io;
use std::io::Write;

/// Shared utilities for shell script generators
pub struct SharedUtils;

impl SharedUtils {
    // Removed unused parse functions to simplify code

    /// Convert glob pattern to regex pattern    // Removed unused convert_extglob_to_regex function

    // Removed unused expand_brace_expression function

    // Removed unused escape_string_for_language function

    /// Generate indentation string    // Removed unused extract_var_name function

    /// Read file content with lossy UTF-8 decoding (replaces invalid byte sequences)
    pub fn read_file_lossy(path: &str) -> io::Result<String> {
        let bytes = fs::read(path)?;
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    /// Read a file as a byte stream and convert to a String that PRESERVES
    /// every invalid byte as a private-use marker char (U+E000 + byte).
    /// bash treats scripts as byte streams (echo of a non-UTF-8 byte passes
    /// it through), so the Perl generator converts the markers back to
    /// `\xNN` byte escapes for byte-exact output.
    pub fn read_file_lossy_marked(path: &str) -> io::Result<String> {
        let bytes = fs::read(path)?;
        Ok(Self::bytes_to_marked_lossy(&bytes))
    }

    pub fn bytes_to_marked_lossy(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            match std::str::from_utf8(&bytes[i..]) {
                Ok(s) => {
                    out.push_str(s);
                    break;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    if valid > 0 {
                        out.push_str(std::str::from_utf8(&bytes[i..i + valid]).unwrap());
                        i += valid;
                    }
                    // Map ONE invalid byte to a PUA marker (byte streams:
                    // bash never groups bytes).
                    let b = bytes[i];
                    out.push(char::from_u32(0xE000 + b as u32).unwrap());
                    i += 1;
                }
            }
        }
        out
    }

    /// Write content to file with proper UTF-8 encoding
    pub fn write_utf8_file(path: &str, content: &str) -> io::Result<()> {
        // Write UTF-8 content without BOM for better shell compatibility
        let mut file = fs::File::create(path)?;
        file.write_all(content.as_bytes())?;
        file.flush()?;
        Ok(())
    }

    /// Check if a string looks like a variable name
    pub fn is_variable_name(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }

        let first_char = s.chars().next().unwrap();
        if !first_char.is_alphabetic() && first_char != '_' {
            return false;
        }

        s.chars().all(|c| c.is_alphanumeric() || c == '_')
    }

    /// Convert shell arithmetic operators to language-specific equivalents
    pub fn convert_arithmetic_operators(expr: &str, language: &str) -> String {
        let mut result = expr.to_string();

        // Common arithmetic operators that are usually the same
        let operators = ["++", "--", "+=", "-=", "*=", "/=", "%=", "**="];
        for op in &operators {
            result = result.replace(op, op);
        }

        // Handle variable references based on language
        match language {
            "perl" => {
                // Ensure $ prefix for single identifiers
                // Split by operators, not just whitespace
                let operators = ['+', '-', '*', '/', '%', '(', ')', ' ', '\t', '\n'];
                let parts: Vec<&str> = result.split(|c| operators.contains(&c)).collect();
                let mut final_result = String::new();
                let mut last_pos = 0;

                for part in parts {
                    let part = part.trim();
                    if !part.is_empty() {
                        // Find where this part appears in the original string
                        if let Some(pos) = result[last_pos..].find(part) {
                            // Add any operators that come before this part
                            let actual_pos = last_pos + pos;
                            if actual_pos > last_pos {
                                final_result.push_str(&result[last_pos..actual_pos]);
                            }

                            // Add the part (with $ prefix if it's a variable)
                            if Self::is_variable_name(part) {
                                final_result.push_str(&format!("${}", part));
                            } else {
                                final_result.push_str(part);
                            }

                            last_pos = actual_pos + part.len();
                        }
                    }
                }

                // Add any remaining characters
                if last_pos < result.len() {
                    final_result.push_str(&result[last_pos..]);
                }

                final_result
            }
            "rust" => {
                // Rust variables don't need special prefix in expressions
                result
            }
            _ => result,
        }
    }
}
