fn main() {
    let src = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let cmds = debashl::Parser::new(&src).parse().unwrap();
    for c in &cmds {
        match c {
            debashl::Command::Simple(sc) => {
                for a in &sc.args {
                    println!("WORD: {:?}", a);
                }
            }
            other => println!("CMD: {:?}", other),
        }
    }
}
