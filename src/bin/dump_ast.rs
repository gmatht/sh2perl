// dump_ast — print the RAW parser AST for a shell script.
//
// Consumed by harness/check_ast.pl, which pins parser distinguishability
// properties that the downstream transforms must not paper over (e.g.
// `echo x $$` vs `echo x$$`). It parses and prints; it runs no transform, so
// what it shows is the parser's own output.
//
// Format (what the harness greps for):
//   CMD:  <Debug of the command>          — one line per top-level command
//   WORD: <Debug of each simple-command arg> — the `^WORD:` count is the
//          argument count the parser produced.
//
// History: this bin was deleted with the old debashc/dump-bins layer in
// 4d5d2d2c6, which left check_ast.pl unable to build its own falsifier (the
// AST half of the script could not run at all). Restored here, same contract.

use debashl::{Command, Parser};

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: dump_ast <script.sh>");
            std::process::exit(2);
        }
    };
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("dump_ast: read {}: {}", path, e);
        std::process::exit(2);
    });
    let cmds = Parser::new(&src).parse().unwrap_or_else(|e| {
        // A parse error is a legitimate answer for a harness case that
        // expects one (beh_unterminated_param_parse_error); say so on stdout
        // so the case sees text rather than an empty string.
        println!("PARSE ERROR: {:?}", e);
        std::process::exit(0);
    });
    for c in &cmds {
        println!("CMD: {:?}", c);
        if let Command::Simple(sc) = c {
            for a in &sc.args {
                println!("WORD: {:?}", a);
            }
        }
    }
}
