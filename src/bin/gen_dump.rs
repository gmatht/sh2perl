use debashl::parser::commands::parse_commands_from_text;
fn main() {
    let src = std::env::args().nth(1).unwrap();
    let cmds = parse_commands_from_text(&src).expect("parse");
    let mut gen = debashl::generator::Generator::new();
    let perl = gen.generate(&cmds);
    println!("{perl}");
}