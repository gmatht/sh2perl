use debashl::ast::Word;
use debashl::Generator;
use debashl::Parser;

fn show_generated_for_shell(src: &str) {
    println!("\n--- Generating for shell: {} ---\n", src);
    match Parser::new(src).parse() {
        Ok(cmds) => {
            let mut gen = Generator::new();
            let perl = gen.generate(&cmds);
            println!("Generated Perl:\n{}", perl);
        }
        Err(e) => println!("Parse error: {}", e),
    }
}

fn main() {
    let mut gen = Generator::new();

    let cases = vec![
        "(echo \"VAR1: $VAR1, VAR2: $VAR2\")",
        "VAR1='value1' VAR2='value2' echo \"VAR1: $VAR1, VAR2: $VAR2\"",
        "(echo 'single quotes $VAR')",
        "simple_lit",
        "contains{brace}and]",
    ];

    for c in cases {
        let w = Word::literal(c.to_string());
        let out = gen.perl_string_literal_no_interp(&w);
        println!("INPUT: {}\n -> {}\n", c, out);
    }

    // Try generating full Perl for a small shell snippet that contains the problematic form
    let shell_line = "x=`(VAR1='value1' VAR2='value2' echo \"VAR1: $VAR1, VAR2: $VAR2\")`";
    show_generated_for_shell(shell_line);
}
