//! Backend node manifests + a core planner (the "solve" idea).
//!
//! Design: `docs/shir-primitives.md` §"Backend node manifests + a core
//! planner". Each backend declares the shIR node types it renders natively
//! in a plain `nodes.txt` (one per line; `#` comments; `sh2.*` = the
//! runtime fallback namespace). The core keeps a candidate table per shell
//! command and SOLVES a reduction into exactly the backend's declared
//! nodes — picking the most natural (lowest-priority) reachable candidate
//! and recursing into composite nodes.
//!
//! This is the shape `text_ops` grows into: a manifest-driven planner
//! instead of a single hard-coded lowering. This module is the FIRST
//! SLICE — the manifest loader + candidate table + solver, unit-tested
//! against the three-manifest scenario from the design doc. Wiring it into
//! `text_ops` (and per-backend transform selection) is the follow-up.
//!
//! Correctness note: the candidate table and reductions here are the
//! MECHANISM, not the final semantics. The off-by-one `Split+ArrayLen`
//! `wc -l` candidate and the `RegCount → Split+ArrayLen` reduction are
//! placeholders for the exact per-backend compositions (a char-loop count
//! is exact; split+len is not). Each candidate must be corpus-proven
//! byte-exact per backend before it is offered (the doc's extended
//! guardrail).

use std::collections::HashSet;
use std::io;
use std::path::Path;

use crate::ir::IrExpr;
use crate::shir_nodes::{ArrayLen, RegCount, Split};

/// A backend's declared native node set, loaded from `backends/<lang>/nodes.txt`.
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub nodes: HashSet<String>,
    pub has_sh2: bool,
}

impl Capabilities {
    /// Parse a nodes.txt manifest: one node type per line, `#` comments,
    /// `sh2.*` marks the runtime fallback namespace.
    pub fn from_manifest(text: &str) -> Self {
        let mut cap = Capabilities::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "sh2.*" {
                cap.has_sh2 = true;
            } else {
                cap.nodes.insert(line.to_string());
            }
        }
        cap
    }

    /// Load a manifest from a file.
    pub fn load(path: &Path) -> io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(Self::from_manifest(&text))
    }

    /// Does the backend render this node natively?
    pub fn supports(&self, node: &str) -> bool {
        self.nodes.contains(node)
    }
}

/// A candidate lowering for a command: the node types it requires and its
/// naturalness priority (lower = more natural).
#[derive(Debug, Clone)]
pub struct Candidate {
    pub name: &'static str,
    pub nodes: &'static [&'static str],
    pub priority: u32,
}

/// The catalogue: command → ordered candidates (lowest priority first).
/// Each candidate is tagged with the node types it requires.
pub fn candidates(cmd: &str) -> Vec<Candidate> {
    match cmd {
        "wc -l" => vec![
            Candidate { name: "RegCount", nodes: &["RegCount"], priority: 0 },
            Candidate {
                name: "SplitArrayLenEndsWith",
                nodes: &["Split", "ArrayLen", "EndsWith"],
                priority: 1,
            },
            Candidate {
                name: "SplitArrayLen",
                nodes: &["Split", "ArrayLen"],
                priority: 2,
            },
            Candidate { name: "sh2.wc", nodes: &["sh2.wc"], priority: 9 },
        ],
        "wc -w" => vec![
            Candidate {
                name: "SplitArrayLen",
                nodes: &["Split", "ArrayLen"],
                priority: 0,
            },
            Candidate { name: "RegCount", nodes: &["RegCount"], priority: 1 },
            Candidate { name: "sh2.wc", nodes: &["sh2.wc"], priority: 9 },
        ],
        "wc -c" => vec![
            Candidate { name: "StrLen", nodes: &["StrLen"], priority: 0 },
            Candidate { name: "sh2.wc", nodes: &["sh2.wc"], priority: 9 },
        ],
        _ => vec![],
    }
}

/// Composite-node reductions: a node that is NOT a leaf can be implemented
/// in terms of other nodes. `RegCount → Split+ArrayLen` is the doc's
/// example (valid for single-char regexes like `\n`; a general regex like
/// `\s+` does NOT reduce — a refinement for a later slice).
fn reductions(node: &str) -> &'static [&'static str] {
    match node {
        "RegCount" => &["Split", "ArrayLen"],
        _ => &[],
    }
}

/// Is `node` reachable from the manifest, directly or by recursing into
/// composite reductions? `sh2.*` nodes are reachable iff the manifest
/// declares the runtime fallback.
fn reachable(node: &str, cap: &Capabilities, seen: &mut HashSet<String>) -> bool {
    if node.starts_with("sh2.") {
        return cap.has_sh2;
    }
    if cap.supports(node) {
        return true;
    }
    if !seen.insert(node.to_string()) {
        return false; // cycle guard
    }
    let red = reductions(node);
    if red.is_empty() {
        return false; // an unsupported leaf is not reachable
    }
    red.iter().all(|sub| reachable(sub, cap, seen))
}

/// The solver: pick the lowest-priority candidate whose required nodes are
/// all reachable from the manifest. Returns the chosen candidate's name,
/// or None → fall back to `sh2.*` / the original command.
pub fn plan(cmd: &str, cap: &Capabilities) -> Option<&'static str> {
    for cand in candidates(cmd) {
        let mut seen = HashSet::new();
        if cand.nodes.iter().all(|n| reachable(n, cap, &mut seen)) {
            return Some(cand.name);
        }
    }
    None
}

/// Embedded per-backend manifests (the "solve" idea, observable end to
/// end). In production these live in `backends/<lang>/nodes.txt` and are
/// loaded via `Capabilities::load`; the embedded table keeps the first
/// demonstration self-contained (the core doesn't know the workspace root).
/// A backend absent here = an empty manifest (falls back to `sh2.*`).
pub fn manifest_for(target: &str) -> Capabilities {
    match target {
        // C: no regex — Split+ArrayLen (or a char-loop) for wc -l.
        "c" => Capabilities::from_manifest("Split\nArrayLen\nStrLen\nWordCount\n"),
        // JS/estree/perl: regex-capable — RegCount is the natural wc -l.
        "js" | "estree" | "perl" | "go" | "rs" | "zig" | "java" | "python" => {
            Capabilities::from_manifest("RegCount\nSplit\nArrayLen\nStrLen\nWordCount\n")
        }
        _ => Capabilities::default(),
    }
}

/// Build the IR for `wc -l` given the input text and the target backend's
/// manifest. Internally runs the solver; the chosen candidate is built
/// with composites EXPANDED to the backend's declared leaves — so a
/// no-regex backend (C) that reaches `RegCount` only via recursion gets
/// `ArrayLen(Split(text, "\n"))`, never a `RegCount` node it can't render.
/// Returns None for the `sh2.wc` fallback (keep the original command).
pub fn build_wc_l(text: IrExpr, cap: &Capabilities) -> Option<IrExpr> {
    for cand in candidates("wc -l") {
        let mut seen = HashSet::new();
        if cand.nodes.iter().all(|n| reachable(n, cap, &mut seen)) {
            return build_candidate(cand.name, text, cap);
        }
    }
    None
}

fn build_candidate(name: &str, text: IrExpr, cap: &Capabilities) -> Option<IrExpr> {
    match name {
        // Regex backend: RegCount(text, /\n/) — a newline count, exact.
        "RegCount" => {
            if cap.supports("RegCount") {
                Some(IrExpr::Ext(Box::new(RegCount {
                    text,
                    pattern: "\\n".to_string(),
                })))
            } else {
                // Reached via recursion (no-regex backend): expand to the
                // doc's Split+ArrayLen composition (off-by-one candidate,
                // only offered to backends that accept it).
                Some(IrExpr::Ext(Box::new(ArrayLen {
                    array: IrExpr::Ext(Box::new(Split {
                        text,
                        delim: "\\n".to_string(),
                        is_regex: false,
                        in_place: false,
                    })),
                })))
            }
        }
        // No-regex backend, explicit candidate: ArrayLen(Split(text, "\n")).
        "SplitArrayLen" => Some(IrExpr::Ext(Box::new(ArrayLen {
            array: IrExpr::Ext(Box::new(Split {
                text,
                delim: "\\n".to_string(),
                is_regex: false,
                in_place: false,
            })),
        }))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(manifest: &str) -> Capabilities {
        Capabilities::from_manifest(manifest)
    }

    #[test]
    fn manifest_parses_comments_and_sh2_marker() {
        let c = cap(
            "# Perl renderer\n\
             StrLen\n\
             Split\n\
             ArrayLen\n\
             sh2.*\n",
        );
        assert!(c.supports("StrLen"));
        assert!(c.supports("Split"));
        assert!(c.supports("ArrayLen"));
        assert!(!c.supports("RegCount"));
        assert!(c.has_sh2);
    }

    #[test]
    fn regex_backend_picks_regcount_for_wc_l() {
        // JS / Perl: manifest has RegCount → the natural p0 candidate.
        let js = cap("RegCount\nSplit\nArrayLen\n");
        assert_eq!(plan("wc -l", &js), Some("RegCount"));
        let perl = cap("RegCount\n");
        assert_eq!(plan("wc -l", &perl), Some("RegCount"));
    }

    #[test]
    fn no_regex_backend_recurses_regcount_to_split_arraylen() {
        // C: no regex, but Split+ArrayLen → RegCount reduces to them.
        let c = cap("Split\nArrayLen\n");
        assert_eq!(plan("wc -l", &c), Some("RegCount"));
    }

    #[test]
    fn wc_c_picks_strlen() {
        let c = cap("StrLen\n");
        assert_eq!(plan("wc -c", &c), Some("StrLen"));
    }

    #[test]
    fn empty_manifest_falls_back_to_none() {
        let c = cap("");
        assert_eq!(plan("wc -l", &c), None);
    }

    #[test]
    fn sh2_fallback_is_reachable_only_when_declared() {
        // No primitives, but sh2.* declared → the sh2.wc fallback candidate.
        let with_sh2 = cap("sh2.*\n");
        assert_eq!(plan("wc -l", &with_sh2), Some("sh2.wc"));
        // No sh2.* → nothing reachable.
        let bare = cap("StrLen\n");
        assert_eq!(plan("wc -l", &bare), None);
    }

    #[test]
    fn unknown_command_has_no_candidates() {
        let c = cap("RegCount\n");
        assert_eq!(plan("ls -la", &c), None);
    }

    #[test]
    fn wc_l_builds_different_ir_per_backend() {
        use crate::ir::StrStyle;
        // The "solve" idea, observable: the SAME `wc -l` input lowers to
        // DIFFERENT IR depending on the target backend's manifest.
        let text = IrExpr::Str("a\nb\n".to_string(), StrStyle::DoubleQuoted);
        // JS has RegCount → the natural RegCount composition.
        let js = manifest_for("js");
        let js_ir = build_wc_l(text.clone(), &js).expect("js builds IR");
        // C has no regex → RegCount reached only via recursion, so it is
        // EXPANDED to ArrayLen(Split(text, "\n")) — never a RegCount node.
        let c = manifest_for("c");
        let c_ir = build_wc_l(text.clone(), &c).expect("c builds IR");
        // Same input, different backends → different IR.
        assert_ne!(format!("{js_ir:?}"), format!("{c_ir:?}"));
        // JS gets a RegCount; C gets an ArrayLen(Split) composition.
        assert!(format!("{js_ir:?}").contains("RegCount"));
        assert!(format!("{c_ir:?}").contains("ArrayLen"));
        assert!(!format!("{c_ir:?}").contains("RegCount"));
    }

    #[test]
    fn manifest_for_unknown_target_is_empty() {
        let m = manifest_for("glsl");
        assert!(m.nodes.is_empty());
        assert!(!m.has_sh2);
    }
}
