use debashl::{Generator, Parser};

#[test]
fn subshell_with_env_assignments_generates_non_interpolating_literal() {
    let src = "x=`(VAR1='value1' VAR2='value2' echo \"VAR1: $VAR1, VAR2: $VAR2\")`";
    let cmds = Parser::new(src)
        .parse()
        .expect("Failed to parse test shell snippet");

    let mut gen = Generator::new();
    let perl = gen.generate(&cmds);

    // The generated Perl should embed the inner shell command as a
    // non-interpolating literal (q{...}) so that $VAR sequences are not
    // interpreted by Perl at compile time.
    assert!(
        perl.contains("q{(VAR1='value1' VAR2='value2' echo \"VAR1: $VAR1, VAR2: $VAR2\")}"),
        "Generated Perl did not contain expected q{{...}} literal.\nGenerated:\n{}",
        perl
    );
}
