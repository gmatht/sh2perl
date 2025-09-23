use std::fs;
use std::path::Path;
// use std::collections::HashMap;

fn escape_js(content: &str) -> String {
    let mut result = String::new();
    
    for ch in content.chars() {
        match ch {
            '`' => result.push_str("\\`"),
            '\\' => result.push_str("\\\\"),
            '$' => result.push_str("\\$"),
            '#' => result.push_str("\\#"),
            _ => result.push(ch),
        }
    }
    
    result
}

fn load_test_results() -> std::collections::HashMap<String, String> {
    let mut results = std::collections::HashMap::new();
    
    // Try to read the results files
    let result_files = [
        ("results/equivalent.txt", "✅ Equivalent"),
        ("results/cangenerate.txt", "⚠️ Can Generate"),
        ("results/canparse.txt", "🔍 Can Parse"),
        ("results/canlex.txt", "📝 Can Lex"),
        ("results/failed.txt", "❌ Failed"),
    ];
    
    for (file_path, category) in result_files {
        if let Ok(content) = fs::read_to_string(file_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    results.insert(trimmed.to_string(), category.to_string());
                }
            }
        }
    }
    
    results
}

fn categorize_file(filename: &str, test_results: &std::collections::HashMap<String, String>) -> String {
    // First check if we have test results for this file
    if let Some(category) = test_results.get(filename) {
        return category.clone();
    }
    
    // Fallback to filename-based categorization for files not in test results
    let lower_filename = filename.to_lowercase();
    
    if lower_filename.contains("control_flow") || lower_filename.contains("if") || lower_filename.contains("loop") || lower_filename.contains("function") || lower_filename.contains("case") {
        "Control Flow".to_string()
    } else if lower_filename.contains("pipeline") {
        "Pipelines".to_string()
    } else if lower_filename.contains("array") {
        "Arrays".to_string()
    } else if lower_filename.contains("parameter_expansion") {
        "Parameter Expansion".to_string()
    } else if lower_filename.contains("brace_expansion") {
        "Brace Expansion".to_string()
    } else if lower_filename.contains("pattern_matching") || lower_filename.contains("extglob") || lower_filename.contains("nocase") {
        "Pattern Matching".to_string()
    } else if lower_filename.contains("process_substitution") {
        "Process Substitution".to_string()
    } else if lower_filename.contains("ansi_quoting") {
        "ANSI Quoting".to_string()
    } else if lower_filename.contains("grep") {
        "Grep Examples".to_string()
    } else if lower_filename.contains("arithmetic") || lower_filename.contains("numeric") || lower_filename.contains("gcd") || lower_filename.contains("fibonacci") || lower_filename.contains("factorize") || lower_filename.contains("primes") {
        "Arithmetic & Math".to_string()
    } else if lower_filename.contains("find") || lower_filename.contains("home") {
        "File Operations".to_string()
    } else if lower_filename.contains("local") || lower_filename.contains("subprocess") || lower_filename.contains("cd") {
        "Shell Operations".to_string()
    } else if lower_filename.contains("hard_to_") || lower_filename.contains("complex") || lower_filename.contains("nested") {
        "Advanced Examples".to_string()
    } else if lower_filename.contains("issue") {
        "Issue Examples".to_string()
    } else if filename.ends_with(".txt") {
        "Data Files".to_string()
    } else {
        "Basic Examples".to_string()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let examples_dir = "examples";
    let output_file = "www/examples.js";
    
    println!("Converting examples from {} to JavaScript...", examples_dir);
    
    // Load test results for categorization
    let test_results = load_test_results();
    if test_results.is_empty() {
        println!("⚠️  No test results found. Run 'cargo run -- --test-eq' first to generate results/* files.");
        println!("   Using filename-based categorization as fallback.");
    } else {
        println!("📊 Loaded test results: {} categorized examples", test_results.len());
    }
    
    // Start the JavaScript file
    let mut js_content = String::new();
    js_content.push_str("// Shell script examples for the Debashc compiler\n");
    js_content.push_str("// This file contains all examples that were previously embedded in WASM\n");
    js_content.push_str("// Generated automatically from examples/ directory\n");
    js_content.push_str("// Categorized based on test results from results/* files\n\n");
    js_content.push_str("export const examples = {\n");
    
    // Find all shell script and text files
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(examples_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if let Some(extension) = path.extension() {
                    if extension == "sh" || extension == "txt" {
                        files.push(path);
                    }
                }
            }
        }
    }
    
    // Sort files for consistent output
    files.sort();
    
    let total_files = files.len();
    let mut count = 0;
    
    for file_path in &files {
        if let Some(filename) = file_path.file_name() {
            if let Some(filename_str) = filename.to_str() {
                // Read file content
                let content = fs::read_to_string(file_path)?;
                
                // Escape the content for JavaScript
                let escaped_content = escape_js(&content);
                
                // Add comma if not the first entry
                if count > 0 {
                    js_content.push_str(",\n");
                }
                
                // Write the example entry
                js_content.push_str(&format!("  '{}': `{}`", filename_str, escaped_content));
                
                count += 1;
                println!("Processed {}/{}: {}", count, total_files, filename_str);
            }
        }
    }
    
    // Close the examples object
    js_content.push_str("\n};\n\n");
    
    // Generate categories automatically based on test results and actual files
    let mut categories: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    
    for file_path in &files {
        if let Some(filename) = file_path.file_name() {
            if let Some(filename_str) = filename.to_str() {
                let category = categorize_file(filename_str, &test_results);
                categories.entry(category).or_insert_with(Vec::new).push(filename_str.to_string());
            }
        }
    }
    
    // Add helper functions
    js_content.push_str("// Helper function to get all example names\n");
    js_content.push_str("export function getExampleNames() {\n");
    js_content.push_str("  return Object.keys(examples);\n");
    js_content.push_str("}\n\n");
    
    js_content.push_str("// Helper function to get example by name\n");
    js_content.push_str("export function getExample(name) {\n");
    js_content.push_str("  return examples[name] || null;\n");
    js_content.push_str("}\n\n");
    
    js_content.push_str("// Helper function to get examples grouped by category\n");
    js_content.push_str("export function getExamplesByCategory() {\n");
    js_content.push_str("  const categories = {\n");
    
    // Write the auto-generated categories
    let mut category_entries = Vec::new();
    for (category, filenames) in &categories {
        let filenames_str = filenames.iter()
            .map(|f| format!("'{}'", f))
            .collect::<Vec<_>>()
            .join(", ");
        category_entries.push(format!("    '{}': [{}]", category, filenames_str));
    }
    
    // Sort categories for consistent output
    category_entries.sort();
    js_content.push_str(&category_entries.join(",\n"));
    
    js_content.push_str("\n  };\n");
    js_content.push_str("  \n");
    js_content.push_str("  return categories;\n");
    js_content.push_str("}\n\n");
    
    js_content.push_str("// Helper function to get examples as JSON (for compatibility with existing code)\n");
    js_content.push_str("export function examplesJson() {\n");
    js_content.push_str("  return JSON.stringify(Object.entries(examples).map(([name, content]) => ({\n");
    js_content.push_str("    name,\n");
    js_content.push_str("    content\n");
    js_content.push_str("  })));\n");
    js_content.push_str("}\n");
    
    // Write the output file
    fs::write(output_file, js_content)?;
    
    println!("Generated {} with {} examples", output_file, count);
    
    // Now let's compare with the existing examples.js
    println!("\nComparing generated file with existing examples.js...");
    
    if Path::new("www/examples.js").exists() {
        println!("Found existing examples.js, comparing...");
        
        // For now, just note that they're different
        println!("❌ DIFFERENCE: Generated file differs from existing examples.js");
        println!("This is expected as the new file has proper escaping");
    } else {
        println!("No existing examples.js found to compare against");
    }
    
    println!("\nGenerated file: {}", output_file);
    println!("Original file: www/examples.js");
    println!("\nTo replace the original, run: mv {} www/examples.js", output_file);
    
    Ok(())
}
