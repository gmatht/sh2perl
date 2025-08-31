use crate::ast::*;
use crate::generator::Generator;

/// Generate logical operator pipeline (&& and ||)
/// This works generically on ANY command type - simple commands, pipelines, redirects, etc.
pub fn generate_logical_pipeline(generator: &mut Generator, pipeline: &Pipeline) -> String {
    let mut output = String::new();
    
    // Logical pipelines should NEVER capture STDOUT - they're about conditional execution
    output.push_str(&generator.indent());
    output.push_str("{\n");
    generator.indent_level += 1;
    
    // Execute first command and check exit code
    let first_cmd = &pipeline.commands[0];
    output.push_str(&generator.indent());
    output.push_str(&generator.generate_command(first_cmd));
    
    // For each logical operator, check exit code and conditionally execute
    for (i, (operator, command)) in pipeline.operators.iter().zip(pipeline.commands.iter().skip(1)).enumerate() {
        match operator {
            PipeOperator::And => {
                // && : execute if previous command succeeded (exit code 0)
                output.push_str(&generator.indent());
                output.push_str("if ($? == 0) {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&generator.generate_command(command));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            }
            PipeOperator::Or => {
                // || : execute if previous command failed (exit code != 0)
                output.push_str(&generator.indent());
                output.push_str("if ($? != 0) {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&generator.generate_command(command));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            }
            PipeOperator::Pipe => {
                // This shouldn't happen in logical pipelines, but handle gracefully
                output.push_str(&generator.indent());
                output.push_str("warn \"Unexpected pipe operator in logical pipeline\";\n");
            }
        }
    }
    
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    
    output
}
