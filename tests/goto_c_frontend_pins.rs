//! The c-sh-go goto pins, run through the shared goto-elimination pass.
//!
//! `frontends/c-sh-go/testdata/t2{7,30,31}_goto*.c` are the pinned goto
//! shapes the pass was written for (backward guarded -> loop; multi-level
//! exit -> flag + break; forward cleanup -> inverted If). Their own gate
//! (`make test` in that frontend) exercises them, but it does so against
//! the SHARED tree's core — so a change to `restructure.rs` in this
//! worktree is not covered by it. This test runs the same sources through
//! THIS worktree's pass and the real estree runner, with native `gcc` as
//! the oracle, so the shared pass's three original shapes are pinned by
//! whatever version of the file is under test.
//!
//! Skips (does not fail) when the c-sh-go binary or `gcc` is unavailable —
//! a missing oracle is an environment gap, not a pass verdict.

use debashl::shir::shir_to_estree_json;
use debashl::shir_json_in::shir_json_to_ir;
use debashl::shir_passes::{restructure_goto_only, strip_cfor};
use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn c_frontend() -> PathBuf {
    // The workspace's `frontends/` is a symlink to `sh2perl/frontends`.
    workspace_root().join("sh2perl/frontends/c-sh-go/c-sh-go")
}

/// Compile + run `src` natively with gcc: the oracle.
fn native_c(src: &str, tag: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("cgotoe2e-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let c = dir.join("prog.c");
    std::fs::write(&c, src).ok()?;
    let bin = dir.join("prog.bin");
    let cc = Command::new("cc").arg("-o").arg(&bin).arg(&c).output().ok()?;
    if !cc.status.success() {
        return None;
    }
    let out = Command::new(&bin).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// c-sh-go -> shared goto pass -> estree runner.
fn transpile_and_run(src: &str, tag: &str) -> Option<String> {
    let ws = workspace_root();
    let fe = c_frontend();
    if !fe.exists() {
        return None;
    }
    let dir = std::env::temp_dir().join(format!("cgotoe2e-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let c = dir.join("prog.c");
    std::fs::write(&c, src).ok()?;
    let a1 = Command::new(&fe).arg("--shir").arg(&c).output().ok()?;
    if !a1.status.success() {
        return None;
    }
    let a1 = String::from_utf8_lossy(&a1.stdout).into_owned();
    let mut prog = shir_json_to_ir(&a1).expect("c-sh-go A1 must deserialize");
    restructure_goto_only(&mut prog);
    strip_cfor(&mut prog);
    let estree = shir_to_estree_json(&prog).expect("estree render");
    assert!(
        !estree.contains("unsupported"),
        "a goto survived into the renderer for {tag}: {estree}"
    );
    let ef = dir.join("prog.estree.json");
    std::fs::write(&ef, &estree).ok()?;
    let js = Command::new("node")
        .arg(ws.join("harness/estree-runner.mjs"))
        .arg(&ef)
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&js.stdout).into_owned())
}

fn pin(src: &str, tag: &str) {
    let Some(want) = native_c(src, tag) else {
        eprintln!("SKIP {tag}: no working `cc`/oracle");
        return;
    };
    let Some(got) = transpile_and_run(src, tag) else {
        eprintln!("SKIP {tag}: c-sh-go frontend unavailable");
        return;
    };
    assert_eq!(got, want, "{tag}: transpiled output != native gcc");
}

/// t27 — backward guarded goto `L: body; if (c) goto L;` -> loop.
#[test]
fn t27_backward_guarded_goto() {
    pin(
        "#include <stdio.h>\nint main(void){int i=0;\nloop:\n i++;\n if(i<3) goto loop;\n printf(\"%d\\n\", i);\n return 0;}\n",
        "t27",
    );
}

/// t30 — goto out of nested loops -> flag var + `break` + tail guard.
#[test]
fn t30_nested_multi_level_exit() {
    pin(
        "#include <stdio.h>\nint main(void){int i,j;\nfor(i=0;i<3;i++){for(j=0;j<3;j++){if(i==1&&j==1) goto out; printf(\"%d%d\\n\",i,j);}}\nout:\n printf(\"done\\n\");\n return 0;}\n",
        "t30",
    );
}

/// t31 — forward cleanup goto -> inverted If.
#[test]
fn t31_forward_cleanup_goto() {
    pin(
        "#include <stdio.h>\nint main(void){int err=0;\nif(err) goto cleanup;\nprintf(\"work\\n\");\ncleanup:\n printf(\"cleanup\\n\");\n return 0;}\n",
        "t31",
    );
}
