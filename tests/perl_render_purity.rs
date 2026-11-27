//! A render's Perl must be a PURE FUNCTION of its source.
//!
//! The renderer keeps analysis verdicts (status-write liveness keyed by
//! statement pointers, the type/liveness censuses, …) that are installed for
//! one program and consulted during that program's emission. If any of that
//! state lives at process scope, two renders in one process — concurrent
//! threads in this test binary, an embedder's request loop — read each
//! other's verdicts and emit DIFFERENT Perl for the same input. The stale
//! pointer keys are worse: a later program's statements can be allocated at
//! addresses an earlier verdict set still names, silently eliding (or
//! keeping) status writes in the wrong program.
//!
//! These tests render the same sources repeatedly, interleaved with other
//! renders, and from many threads at once, and require byte-identical text
//! from both Perl entry points (`ir::shir_to_perl` and the shipped
//! `perl_generator_fixed::generate` wrapper).
//!
//! They are deliberately SEPARATE `#[test]` functions running in parallel:
//! the render path is thread-local end to end, so serialising them is not
//! allowed to be what makes them pass.

use debashl::ir::{shir_to_perl, IrProgram};
use std::sync::Arc;

fn to_ir(src: &str) -> IrProgram {
    let (commands, _) = debashl::Parser::new(src).parse_with_lines().expect("parse");
    debashl::shir::ast_to_ir(&commands)
}

fn bare(src: &str) -> String {
    shir_to_perl(&to_ir(src))
}

fn fixed(src: &str) -> String {
    debashl::perl_generator_fixed::generate(&to_ir(src))
}

/// Deep brace/substitution nesting recurses in the parser; give the worker a
/// stack big enough that the test measures render purity, not stack depth.
fn with_big_stack<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("join")
}

/// Sources that cover the renderer's stateful decision points: status
/// liveness (a write that is read vs. dead), command conditions, captures
/// (native and shell-out), and `&&`/`||` chains.
const SCRIPTS: &[&str] = &[
    "cd /no/such\nif [ $? -eq 0 ]; then echo ok; else echo bad; fi\n",
    "cd /no/such\n",
    "cd /tmp && echo x\n",
    "rm -f missing-xyz\nif [ $? -eq 0 ]; then echo ok; else echo bad; fi\n",
    "mkdir d1\necho $?\n",
    "touch f1\ntouch /no/such/dir/t\necho $?\n",
    "printf '%d\\n' zz\necho $?\n",
    "printf '%s\\n' a b c\n",
    "test -n abc\nif [ $? -eq 0 ]; then echo yes; fi\n",
    "[ -z abc ] || echo fallback\n",
    "if grep -q x /etc/hostname; then echo yes; fi\n",
    "if false; then echo a; elif grep -q x /etc/hostname; then echo b; else echo c; fi\n",
    "while grep -q x /etc/hostname; do echo x; break; done\n",
    "n=0\nuntil [ $n -eq 1 ]; do n=$((n+1)); done\necho $n\n",
    "x=`basename $(pwd)`\necho \"$x\"\n",
    "y=world\nz=`echo $y`\necho \"$z\"\n",
    "greeting=hi\nx=$(echo \"$greeting\"; echo second)\necho \"$x\"\n",
    "false && echo hi\ntrue || echo hi\n",
    "false; echo $?\n",
    "x=$(( 1 > 0 ? 10 : 20 ))\necho $x\n",
    "for i in 1 2 3; do printf '%s\\n' $i; done\n",
    "f() { local x=1; echo $x; }\nf\n",
    "echo hi | tr a-z A-Z\n",
    // Emission-SITE counters: background pids, herestring handles, redirect
    // filehandles, capture scratch files, transform scratch variables.
    "sleep 0.01 &\necho bg=$!\nwait\n",
    "cat <<DOC\nhi\nDOC\n",
    "while read -r l; do echo \"L:$l\"; done <<IN\na\nb\nIN\n",
    "cat > /tmp/sh2_purity_out.txt <<DOC\nx\nDOC\n",
    "x=$(echo one)\ny=$(echo two)\necho \"$x$y\"\n",
    "echo hi > /tmp/sh2_purity_a.txt\nls /tmp/sh2_purity_a.txt\n",
    "case x in x) false;; esac\necho $?\n",
];

fn corpus_sample() -> Vec<(String, String)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/examples");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("examples dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".sh"))
        .collect();
    names.sort();
    // Every 40th example: a broad sample without making this test slow.
    // The `parse-*` fixtures are deliberately MALFORMED inputs (that is what
    // they pin); they are not renderable and have no place here.
    names
        .into_iter()
        .step_by(40)
        .filter(|n| !n.starts_with("parse-"))
        .filter_map(|n| std::fs::read_to_string(format!("{dir}/{n}")).ok().map(|s| (n, s)))
        .filter(|(_, s)| debashl::Parser::new(s).parse().is_ok())
        .collect()
}

/// The same source, rendered twice in a row and interleaved with a DIFFERENT
/// source, must give the same text — and so must the shipped wrapper.
#[test]
fn repeated_and_interleaved_renders_are_identical() {
    let mut failures = String::new();
    for (i, src) in SCRIPTS.iter().enumerate() {
        let b1 = bare(src);
        let f1 = fixed(src);
        // Render a different program in between: that is what used to make
        // the next render differ (an installed verdict set surviving).
        let other = SCRIPTS[(i + 1) % SCRIPTS.len()];
        let _ = bare(other);
        let _ = fixed(other);
        let b2 = bare(src);
        let f2 = fixed(src);
        if b1 != b2 {
            failures.push_str(&format!("script {i}: shir_to_perl differs on the 2nd render\n"));
        }
        if f1 != f2 {
            failures.push_str(&format!("script {i}: generate differs on the 2nd render\n"));
        }
        // And through the OTHER entry point's interference.
        if bare(src) != b1 {
            failures.push_str(&format!("script {i}: shir_to_perl drifts after generate\n"));
        }
        if fixed(src) != f1 {
            failures.push_str(&format!("script {i}: generate drifts after shir_to_perl\n"));
        }
    }
    assert!(failures.is_empty(), "{failures}");
}

/// Eight threads rendering every script at once, each compared against the
/// sequential baseline. This is the case the process-global verdict store
/// could not survive: thread A installs its liveness set, thread B consults
/// it before A has finished emitting.
#[test]
fn concurrent_renders_are_identical() {
    let baseline: Arc<Vec<(String, String)>> = Arc::new(
        SCRIPTS
            .iter()
            .map(|s| (bare(s), fixed(s)))
            .collect(),
    );
    let mut handles = Vec::new();
    for t in 0..8 {
        let baseline = Arc::clone(&baseline);
        handles.push(std::thread::spawn(move || {
            let mut failures = String::new();
            for round in 0..3 {
                for (i, src) in SCRIPTS.iter().enumerate() {
                    // Offset each thread's order so the renders interleave
                    // at different points.
                    let i = (i + t * 3) % SCRIPTS.len();
                    let src = SCRIPTS[i];
                    if bare(src) != baseline[i].0 {
                        failures.push_str(&format!(
                            "thread {t} round {round}: shir_to_perl differs for script {i}\n"
                        ));
                    }
                    if fixed(src) != baseline[i].1 {
                        failures.push_str(&format!(
                            "thread {t} round {round}: generate differs for script {i}\n"
                        ));
                    }
                }
            }
            failures
        }));
    }
    let mut failures = String::new();
    for h in handles {
        failures.push_str(&h.join().expect("thread"));
    }
    assert!(failures.is_empty(), "{failures}");
}

/// The same purity property over a sample of REAL corpus programs (the
/// snippets above are small; the corpus exercises the transforms' verdicts).
#[test]
fn corpus_sample_renders_are_identical_concurrently() {
    let sample = corpus_sample();
    assert!(!sample.is_empty(), "no example sample");
    let baseline = Arc::new(with_big_stack({
        let sample = sample.clone();
        move || -> Vec<(String, String)> {
            sample.iter().map(|(_, s)| (bare(s), fixed(s))).collect()
        }
    }));
    let mut handles = Vec::new();
    for t in 0..4 {
        let baseline = Arc::clone(&baseline);
        let sample = sample.clone();
        handles.push(std::thread::Builder::new()
            .stack_size(64 << 20)
            .spawn(move || {
                let mut failures = String::new();
                for (i, (name, src)) in sample.iter().enumerate() {
                    let i = (i + t * 5) % sample.len();
                    let (name, src) = &sample[i];
                    let _ = name;
                    if bare(src) != baseline[i].0 {
                        failures.push_str(&format!(
                            "thread {t}: shir_to_perl differs for {name}\n"
                        ));
                    }
                    if fixed(src) != baseline[i].1 {
                        failures.push_str(&format!("thread {t}: generate differs for {name}\n"));
                    }
                }
                failures
            })
            .expect("spawn"));
    }
    let mut failures = String::new();
    for h in handles {
        failures.push_str(&h.join().expect("thread"));
    }
    assert!(failures.is_empty(), "{failures}");
}
