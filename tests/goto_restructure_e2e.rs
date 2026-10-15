//! goto-restructure end-to-end: real frontend A1 -> shared pass -> estree JS.
//!
//! The unit tests in `shir_passes/restructure.rs` pin the IR shape the pass
//! produces. This test closes the last gap: it feeds the A1 that the GO
//! FRONTEND ACTUALLY EMITS for the `goto` shapes found in this repo's own
//! Go sources (posix-sh-go `default: goto fiDone`, zig-sh-go
//! `goto matched`) through the two real stages the CLI runs —
//! `restructure_goto_only` then `shir_to_estree` — and executes the result
//! under node, comparing stdout to native `go run`.
//!
//! The A1 is produced by invoking the real `go-sh --shir --raw` binary
//! rather than hand-written, so the test cannot drift from the frontend's
//! actual output (a hand-rolled envelope is exactly how a "the pass works"
//! claim gets made against input the frontend never emits).
//!
//! Why not `fail-go-app`: that gate needs an `otranspilerl-cli` built from
//! the shared tree, whose A1-ingress helper is in flight in another
//! worker's working copy. This test builds against THIS worktree only, so
//! it isolates the pass under test from that churn.
//!
//! Skips (does not fail) when `go-sh` or `go` is unavailable, matching the
//! gates' "a missing oracle is not a renderer verdict" rule; the assertion
//! that matters is the pass's own behaviour, verified whenever the
//! toolchain is present.

use debashl::shir::shir_to_estree_json;
use debashl::shir_json_in::shir_json_to_ir;
use debashl::shir_passes::restructure_goto_only;
use std::path::PathBuf;
use std::process::Command;

/// Workspace root: `tests/` lives at `<root>/sh2perl/.goto-fix/tests`, and
/// the Go frontend is a sibling of `sh2perl` under the workspace root.
fn workspace_root() -> PathBuf {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn go_sh() -> PathBuf {
    workspace_root().join("sh2perl/frontends/go-sh/go-sh")
}

/// Render the A1 to the ESTree JSON the CLI emits (not JS — the harness
/// runner turns ESTree into runnable JS, and that runner is the reference
/// executor the gates use, so going through it is what makes this test
/// measure the same thing `fail-go` does).
fn a1_to_estree_json(a1: &str) -> String {
    let mut prog = shir_json_to_ir(a1).expect("the frontend's A1 must deserialize");
    restructure_goto_only(&mut prog);
    shir_to_estree_json(&prog).expect("estree render must succeed")
}

/// The harness that owns the runner: `<workspace>/harness/estree-runner.mjs`.
fn estree_runner() -> PathBuf {
    workspace_root().join("harness/estree-runner.mjs")
}

/// `go-sh --shir <file> --raw` — the A1 the frontend really emits.
fn frontend_a1(src: &str, tag: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("gotoe2e-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&dir).ok()?;
    let go = dir.join("prog.go");
    std::fs::write(&go, src).ok()?;
    if !go_sh().exists() {
        return None;
    }
    let out = Command::new(go_sh())
        .arg("--shir")
        .arg(&go)
        .arg("--raw")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// native `go run` stdout+rc, the oracle.
fn native(src: &str, tag: &str) -> Option<(String, i32)> {
    let dir = std::env::temp_dir().join(format!("gotoe2e-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&dir).ok()?;
    let go = dir.join("prog.go");
    std::fs::write(&go, src).ok()?;
    let out = Command::new("go").arg("run").arg(&go).output().ok()?;
    Some((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    ))
}

/// Run generated ESTree JSON through the harness runner.
fn run_estree(estree_json: &str, tag: &str) -> (String, i32) {
    let dir = std::env::temp_dir().join(format!("gotoe2e-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let f = dir.join("prog.estree.json");
    std::fs::write(&f, estree_json).expect("write estree");
    let out = Command::new("node")
        .arg(estree_runner())
        .arg(&f)
        .output()
        .expect("node must be on PATH");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// The `for { switch { case …: return; default: goto L } L: rest }` shape —
/// posix-sh-go main.go parseCommand's `default: goto fiDone` and
/// zig-sh-go main.go convertFormat's `goto matched`. The goto's target
/// label sits in the SAME list as the goto, inside the enclosing loop.
const GOTO_IN_SWITCH: &str = r#"package main

import "fmt"

func classify(kw string) string {
	for {
		switch kw {
		case "if", "case", "while":
			fmt.Println("keyword")
			return "kw"
		default:
			fmt.Println("not-a-keyword")
			goto additionalArgs
		}
	additionalArgs:
		fmt.Println("additional-args")
		return "other"
	}
}

func main() {
	fmt.Println(classify("foo"))
}
"#;

#[test]
fn goto_in_switch_inside_loop_reaches_the_label_tail() {
    let Some(a1) = frontend_a1(GOTO_IN_SWITCH, "sw") else {
        eprintln!("SKIP: go-sh frontend binary unavailable");
        return;
    };
    let Some((want, want_rc)) = native(GOTO_IN_SWITCH, "sw") else {
        eprintln!("SKIP: no `go` on PATH — oracle unavailable");
        return;
    };
    let estree = a1_to_estree_json(&a1);
    assert!(
        !estree.contains("unsupported"),
        "goto survived into the renderer and would die at runtime: {estree}"
    );
    let (got, rc) = run_estree(&estree, "sw");
    assert_eq!(rc, want_rc, "exit code: js={rc} go={want_rc}; stdout={got:?}");
    assert_eq!(
        got, want,
        "the goto did not reach its label's tail: pre-fix the `default` arm \
         set a flag nothing read, so the loop broke out before \
         \"additional-args\" and the jump never took effect"
    );
}

/// The `for { … goto L }; L: rest` shape — the goto's target is a sibling
/// AFTER the loop. Before the fix the flag was written but never read, so
/// the tail ran with the loop's remaining iterations still pending.
const GOTO_AFTER_LOOP: &str = r#"package main

import "fmt"

func main() {
	for i := 0; i < 5; i++ {
		if i == 2 {
			fmt.Println("two")
			goto done
		}
		fmt.Println(i)
	}
done:
	fmt.Println("end")
}
"#;

#[test]
fn goto_out_of_loop_guards_the_following_tail() {
    let Some(a1) = frontend_a1(GOTO_AFTER_LOOP, "af") else {
        eprintln!("SKIP: go-sh frontend binary unavailable");
        return;
    };
    let Some((want, want_rc)) = native(GOTO_AFTER_LOOP, "af") else {
        eprintln!("SKIP: no `go` on PATH — oracle unavailable");
        return;
    };
    let estree = a1_to_estree_json(&a1);
    assert!(
        !estree.contains("unsupported"),
        "goto survived into the renderer: {estree}"
    );
    let (got, rc) = run_estree(&estree, "af");
    assert_eq!(rc, want_rc, "exit code: js={rc} go={want_rc}; stdout={got:?}");
    assert_eq!(
        got, want,
        "the loop did not exit at the goto: pre-fix the flag was set but \
         never read, so every iteration ran"
    );
}
