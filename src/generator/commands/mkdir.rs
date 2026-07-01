use crate::ast::*;
use crate::generator::Generator;

fn word_text(word: &Word) -> String {
    let text = word.to_string();
    if (text.starts_with('"') && text.ends_with('"'))
        || (text.starts_with('\'') && text.ends_with('\''))
    {
        text[1..text.len() - 1].to_string()
    } else {
        text
    }
}

pub fn generate_mkdir_command(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    let mut output = String::new();

    // mkdir command syntax: mkdir [options] directory...
    let mut create_parents = false;
    let mut verbose = false;
    let mut directories = Vec::new();

    // Parse mkdir options
    for arg in &cmd.args {
        let arg_str = word_text(arg);
        match arg_str.as_str() {
            "-p" | "--parents" => create_parents = true,
            "-v" | "--verbose" => verbose = true,
            _ => {
                if !arg_str.starts_with('-') {
                    directories.push(generator.perl_string_literal(arg));
                }
            }
        }
    }

    if directories.is_empty() {
        output.push_str("croak \"mkdir: missing operand\\n\";\n");
    } else {
        output.push_str(&generator.indent());
        output.push_str("use File::Path qw(make_path);\n");
        if !generator.declared_locals.contains("err") {
            output.push_str(&generator.indent());
            output.push_str("my $err;\n");
            generator.declared_locals.insert("err".to_string());
        }

        for dir in &directories {
            if create_parents {
                output.push_str(&generator.indent());
                output.push_str(&format!("if ( !-d {} ) {{\n", dir));
                generator.indent_level += 1;

                if verbose {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("my $mkdir_target = {};\n", dir));
                    output.push_str(&generator.indent());
                    output.push_str("my @mkdir_verbose_paths;\n");
                    output.push_str(&generator.indent());
                    output.push_str("my $mkdir_prefix = $mkdir_target =~ m{^/} ? '/' : '';\n");
                    output.push_str(&generator.indent());
                    output.push_str("for my $mkdir_component ( split m{/}, $mkdir_target ) {\n");
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str("next if $mkdir_component eq '';\n");
                    output.push_str(&generator.indent());
                    output.push_str("if ( $mkdir_prefix eq '' || $mkdir_prefix eq '/' ) {\n");
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str("$mkdir_prefix .= $mkdir_component;\n");
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                    output.push_str(&generator.indent());
                    output.push_str("else {\n");
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str("$mkdir_prefix .= '/' . $mkdir_component;\n");
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                    output.push_str(&generator.indent());
                    output.push_str(
                        "push @mkdir_verbose_paths, $mkdir_prefix if !-d $mkdir_prefix;\n",
                    );
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                }

                output.push_str(&generator.indent());
                output.push_str(&format!("make_path( {}, {{ error => \\$err }} );\n", dir));
                output.push_str(&generator.indent());
                output.push_str("if ( @{$err} ) {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!(
                    "croak \"mkdir: cannot create directory \" . {} . \": $err->[0]\\n\";\n",
                    dir
                ));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                if verbose {
                    output.push_str(&generator.indent());
                    output.push_str("for my $mkdir_created (@mkdir_verbose_paths) {\n");
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str(
                        "print \"mkdir: created directory '\" . $mkdir_created . \"'\\n\";\n",
                    );
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                }
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            } else {
                output.push_str(&generator.indent());
                output.push_str(&format!("if ( mkdir {} ) {{\n", dir));
                generator.indent_level += 1;
                if verbose {
                    output.push_str(&generator.indent());
                    output.push_str(&format!(
                        "print \"mkdir: created directory '\" . {} . \"'\\n\";\n",
                        dir
                    ));
                }
                output.push_str(&generator.indent());
                output.push_str("}\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("else {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!(
                    "croak \"mkdir: cannot create directory \" . {} . \": File exists\\n\";\n",
                    dir
                ));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            }
        }
    }

    output
}
