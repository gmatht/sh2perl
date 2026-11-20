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

// ── The NODE-side manifest (what text_ops may EMIT per backend) ───────
//
// `manifest_for` above answers "which composition may this backend's wc -l
// become?". The table below answers the complementary question the §11.12
// design names as the OTHER half: "may the core hand this backend a rich
// Ext node AT ALL — or must it keep the original command?".
//
// It is the authoritative list of what each backend renders natively, kept
// in sync with the renderer reality (verified by `cargo test` against the
// shared handler registry: `planner::tests::node_manifest_matches_renderers`
// asserts every declared node has a handler that renders it).
//
// Why it matters: `text_ops` REPLACES a shell command with an Ext node. On
// a backend that cannot render the node, that replacement is a REGRESSION
// (the command was exact; the node becomes a `sh2.*` stub or a hard
// refusal). Gating the transform on this table is what makes the core
// "lower to supported nodes" instead of emitting unsupported ones.
/// Every target name [`node_manifest`] answers for. Kept as one list so a
/// test can ENUMERATE the offering backends instead of hardcoding them:
/// adding a target here (plus its manifest entry) automatically subjects
/// it to `offering_backends_render_pattern_trims_natively`.
pub const TARGETS: &[&str] = &[
    "perl", "estree", "js", "python", "java", "c", "go", "rs", "zig",
];

pub fn node_manifest(target: &str) -> &'static [&'static str] {
    // Verified native on the shared registry + the backend's own Ext arms.
    // `ALL` = everything the shared handlers render for that backend.
    const ALL_TARGETS: &[&str] = &[
        "StrLen", "WordCount", "CaseTransform", "CharTranslate", "FieldExtract", "RegSub",
        "PathName", "SubStrExtract", "TakeLines", "RegCount", "Split", "ArrayLen", "StringTrim",
        "StringContains", "StringAffix", "AffixStrip", "CutsetTrim", "RepeatStr", "RegexpFind",
    ];
    match target {
        // Full native support (verified by the renderer arms).
        "perl" | "estree" | "js" | "python" => ALL_TARGETS,
        // Go renders the common set natively; the shared handlers' Go arms
        // are TODO for the rest (see all_nodes.rs), so text_ops must not
        // emit those to Go either.
        "go" => &[
            "StrLen", "WordCount", "CaseTransform", "FieldExtract", "StringContains",
            "StringAffix", "StringTrim", "AffixStrip", "CutsetTrim", "Split", "ArrayLen",
            "RegCount",
        ],
        // C renders StrLen + WordCount natively; everything else is a
        // `TODO(unsupported)` marker today.
        "c" => &["StrLen", "WordCount"],
        // Java: the shared registry now has Java arms for the ALL set, but
        // the TakeLines/RegSub/RegCount/FieldExtract/PathName arms emit
        // calls into Java runtime helpers (shTakeLines, shRegSub, shRegCount,
        // shCutField, shBasename/shDirname) that must exist in the Java
        // preamble. Until those land, only the self-contained forms are
        // declared — the rest keeps the original command.
        "java" => &[
            "StrLen", "WordCount", "CaseTransform", "CharTranslate", "Split", "ArrayLen",
            "StringTrim", "StringContains", "StringAffix", "AffixStrip", "RepeatStr",
            "SubStrExtract",
        ],
        // Rust: the text nodes `rust_backend`'s own Ext arms spell in `std`
        // (StringContains/StrLen/CaseTransform/FieldExtract plus
        // `ext_std_str`/`ext_std_num`/the `words_expr` Split arm). Every node
        // declared here is pinned by `offered_text_nodes_execute_like_bash`
        // (or `constructed_text_nodes_execute_like_bash` for a node with no
        // shell-source shape), which compiles the render with `rustc` and
        // compares stdout + exit status with bash byte-for-byte.
        //
        // NOT declared, with the reason (each keeps its original command, so
        // the existing path is untouched):
        //  * RegSub, RegexpFind — `std` has no regex ENGINE: the nodes' specs
        //    are over a COMPILED matcher (POSIX BRE/ERE with backreferences,
        //    greedy `*`, character classes), and a hand-rolled matcher would
        //    be exact only on a subset while rendering silently wrong on the
        //    rest. `sed s///`, `grep -o RE` and `${v/pat/rep}` therefore keep
        //    the command. (The regex the RENDERER does use comes from the
        //    `regex` crate in the emitted Cargo project — that is not `std`,
        //    so it does not license these nodes here.)
        //  * AffixStrip — `${v#p}`/`${v%s}` lower through the backend's own
        //    strip helpers (`__sh_strippre`/`__sh_stripsuf` over
        //    `__sh_fnmatch`), which accept ARBITRARY glob patterns; `std`'s
        //    `strip_prefix`/`strip_suffix` are exact only for the literal
        //    subset, so the node stays out and the shapes keep that path.
        //
        // DECLARED but CONSTRUCT-ONLY (no transform in this pipeline builds
        // them from shell source, so they are pinned by constructing the node
        // and running it against bash, `constructed_text_nodes_execute_like_bash`):
        // StringAffix and CutsetTrim (`[[ $x == P* ]]` renders `.starts_with`
        // in the test lowering directly; the `${s##*[cutset]}` shapes stay
        // shell-out), SortLines and UniqLines (no by-line filter lowering
        // here yet), and RepeatStr outside `yes | head`.
        //
        // WHO OWNS WHAT, so a source never rides a node built for another:
        // CaseTransform is the BASH PARAMETER casing (`${v,,}`/`${v^^}`), which
        // case-maps CODE POINTS; `tr`'s case pairs are the BYTE map and lower to
        // CharTranslate (measured: `tr '[:lower:]' '[:upper:]'` leaves `É`
        // alone, `to_uppercase()` does not).
        "rs" | "rust" => &[
            "StringContains", "StrLen", "CaseTransform", "FieldExtract",
            // fs-lift (class 5/6): the file predicates whose `std` reading is
            // exact — dev+ino identity, full-precision mtime comparison with
            // bash's missing-operand rule, and `-N` as mtime > atime.
            "FsSameFile", "FsNewerThan", "FsModifiedSinceRead",
                "WordCount", "RegCount", "TakeLines", "StringTrim",
                "SubStrExtract", "RepeatStr", "Split", "ArrayLen",
                // `basename`/`dirname` commands: text_ops builds PathName
                // only from a real command (a FLAGGED call keeps its
                // command form), never from `${p##*/}`/`${p%/*}` — those
                // are glob strips, which differ on a trailing slash — so
                // the GNU rules the arm spells are the whole contract.
                "PathName",
                // The cutset trim: `trim_matches(|c| cutset.contains(c))`,
                // with `left`/`right` selecting the one-sided form.
                "CutsetTrim",
                // The prefix/suffix PREDICATE: `.starts_with`/`.ends_with`
                // over the node's LITERAL affix (in a bool context) and its
                // 1/0 spelling in a string context.
                "StringAffix",
                // `tr`'s general map: the lowering EXPANDS ranges and classes
                // before the node is built, so `from`/`to` are the map and
                // `chars().map(...)` is exact (delete and squeeze included).
                "CharTranslate",
                // The by-line filters: a stable `sort_by` over the lines
                // (numeric or lexical key, reverse, `-u`) and a run walk for
                // `uniq` (`-s` projection, `-c`'s width-7 prefix).
                "SortLines", "UniqLines",
            ],
        // Zig: the shared Zig arms are TODO for most nodes.
        "zig" => &["StrLen", "Split", "ArrayLen", "StringContains", "StringAffix", "StringTrim"],
        // The shell-like targets: the ORIGINAL COMMAND is the native form
        // (no rich nodes at all) — text_ops must never rewrite them.
        _ => &[],
    }
}

/// May `text_ops` emit its rich Ext nodes for `target`? False when the
/// target's manifest declares no node the transform emits — the core then
/// keeps the ORIGINAL COMMAND (which every backend renders natively),
/// which is the "lower to supported nodes" behaviour.
///
/// An unknown/absent target answers `false`: refusing to emit is always
/// safe (the command form is exact everywhere), whereas emitting a node an
/// unknown backend cannot render is not.
pub fn emits_renderable_nodes(target: &str) -> bool {
    !node_manifest(target).is_empty()
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

    // ── the NODE-side manifest (what text_ops may EMIT) ───────────────

    #[test]
    fn text_ops_is_gated_by_the_node_manifest() {
        // A backend that renders the rich nodes gets the transform...
        assert!(emits_renderable_nodes("perl"));
        assert!(emits_renderable_nodes("js"));
        assert!(emits_renderable_nodes("estree"));
        assert!(emits_renderable_nodes("python"));
        assert!(emits_renderable_nodes("java"));
        assert!(emits_renderable_nodes("c"));
        // ...a shell-like target does NOT: it keeps the ORIGINAL COMMAND,
        // which is exact there and which text_ops can only make worse.
        assert!(!emits_renderable_nodes("sh"));
        assert!(!emits_renderable_nodes("glsl"));
        // An unknown target refuses to emit (a node an unknown backend
        // cannot render is strictly worse than the command form).
        assert!(!emits_renderable_nodes("some-unknown-target"));
    }

    /// Every declared node must actually be renderable by the shared
    /// registry for that backend — otherwise text_ops would emit a node
    /// that becomes a stub (the exact regression the gate exists to stop).
    ///
    /// The check mirrors the registry's own dispatch: a tag has a handler
    /// iff `handlers/<snake(tag)>.rs` exists (`build.rs` scans that name).
    #[test]
    fn node_manifest_names_have_real_renderers() {
        for target in [
            "perl", "estree", "js", "python", "java", "c", "go", "rs", "zig",
        ] {
            for node in node_manifest(target) {
                // The shared expr-node handlers live in `shir_nodes/*.node`
                // with a matching snake_case handler file.
                let snake = {
                    let mut out = String::new();
                    for (i, c) in node.chars().enumerate() {
                        if c.is_uppercase() {
                            if i > 0 {
                                out.push('_');
                            }
                            out.extend(c.to_lowercase());
                        } else {
                            out.push(c);
                        }
                    }
                    out
                };
                let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("src/render_ext_expr/handlers")
                    .join(format!("{snake}.rs"));
                assert!(
                    path.exists(),
                    "{target}: manifest declares {node} but has no handler {}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn every_declared_node_is_a_known_node_type() {
        // The manifest must not name a non-existent node (a typo would
        // silently widen the gate).
        let known = crate::shir_nodes::all_expr_nodes()
            .into_iter()
            .map(|(tag, _)| tag)
            .collect::<std::collections::HashSet<_>>();
        for target in ["perl", "js", "python", "java", "c", "go", "rs", "zig"] {
            for node in node_manifest(target) {
                assert!(
                    known.contains(*node),
                    "{target}: manifest names {node}, which is not a declared expr node"
                );
            }
        }
    }

    // ── the trim/affix render path, per offering backend ──────────────

    /// The node family these four expansions lower through.
    const TRIM_AFFIX_NODES: &[&str] = &["AffixStrip", "CutsetTrim", "StringTrim", "StringAffix"];

    /// An offering backend whose `${v…}` channel is a documented sh2-runtime
    /// STUB rather than a renderer. `zig` has no param rendering at all —
    /// every expansion becomes `sh2Param()` with a `TODO(unsupported) `
    /// marker — so its four forms are pinned to that gap (see the test) and
    /// it is NOT claimed to render the native strip. When the zig param
    /// channel lands, drop it from this list: the test then starts requiring
    /// the native form there too.
    const NO_NATIVE_TRIM: &[&str] = &["zig"];

    /// Render an IR program through a backend named the way the MANIFEST
    /// names it (`rs`/`js` are the manifest's spellings of `rust`/`estree`).
    fn render_for(target: &str, prog: &crate::ir::IrProgram) -> Option<String> {
        match target {
            "perl" => Some(crate::perl_generator_fixed::generate(prog)),
            "python" => Some(crate::python_backend::shir_to_python(prog)),
            "go" => Some(crate::go_backend::shir_to_go(prog)),
            "java" => crate::java_backend::shir_to_java(prog).ok(),
            "rs" | "rust" => Some(crate::rust_backend::shir_to_rust(prog)),
            "zig" => Some(crate::zig_backend::shir_to_zig(prog)),
            "estree" | "js" => crate::shir::shir_to_estree_json(prog).ok(),
            "c" => Some(crate::c_backend::shir_to_c(prog)),
            _ => None,
        }
    }

    /// The substrings that prove a backend took its NATIVE strip/strip-node
    /// path for a pattern trim (rather than a stub, a slice, or a spawn).
    /// A backend may spell a prefix and a suffix trim differently (perl
    /// substitutes for `#`/`##` and scans suffix lengths for `%`/`%%`), so
    /// each entry is a set: any member proves the native path.
    ///
    /// estree's native form IS the `sh2.param` contract op: the estree target
    /// is a program over the shared sh2 runtime, and `harness/estree-contract.mjs`
    /// checks that callee resolves.
    fn native_strip_markers(target: &str) -> &'static [&'static str] {
        match target {
            "perl" => &["=~ s{", "/\\A(?:"],
            "python" => &["sh2_strip("],
            "go" => &["pExp("],
            "java" => &["shStrip"],
            "rs" | "rust" => &["__sh_strip"],
            // Two native forms: a literal-delimiter trim renders inline as
            // `String.prototype.indexOf/lastIndexOf` + `slice`, and a GLOB
            // pattern goes to `sh2.param` (the runtime op this target calls;
            // `harness/estree-contract.mjs` checks the name resolves).
            "estree" | "js" => &[
                "\"name\":\"param\"",
                "\"name\":\"indexOf\"",
                "\"name\":\"lastIndexOf\"",
            ],
            "c" => &["sh2_strip"],
            _ => &[],
        }
    }

    /// `${v#pat}` / `${v##pat}` / `${v%pat}` / `${v%%pat}` — with `:`, `[[`,
    /// `]`, `!`, `*`, `?` and POSIX classes in the PATTERN — must render
    /// through each offering backend's native strip path, with no shell
    /// spawn. The `:` cases are the regression this whole family was fixed
    /// for: the parser used to read the colon as a `:off:len` slice
    /// separator and every backend took a substring of a variable named
    /// `v##*[![`.
    ///
    /// The offering backends are ENUMERATED from the manifest, so a backend
    /// that starts declaring a trim/affix node is covered with no edit here
    /// (and a manifest target with no renderer wired fails loudly).
    #[test]
    fn offering_backends_render_pattern_trims_natively() {
        // (op, pattern) — every listed character class and both colon shapes.
        let forms: &[(&str, &str)] = &[
            ("#", "*[![:space:]]"),
            ("##", "*[![:space:]]"),
            ("%", "[[:space:]]*"),
            ("%%", "[[:space:]]*"),
            ("#", "*:"),
            ("##", "*:"),
            ("%", ":*"),
            ("%%", ":*"),
            ("##", "[[:alpha:]]"),
            ("%%", "[[:alpha:]]"),
            ("#", "?"),
            ("%", "?"),
        ];
        let offering: Vec<&str> = TARGETS
            .iter()
            .copied()
            .filter(|t| {
                TRIM_AFFIX_NODES
                    .iter()
                    .any(|n| node_manifest(t).contains(n))
            })
            .collect();
        assert!(
            offering.len() >= 6,
            "the manifest table shrank — offering backends: {offering:?}"
        );

        for target in offering {
            let markers = native_strip_markers(target);
            if !NO_NATIVE_TRIM.contains(&target) {
                assert!(
                    !markers.is_empty(),
                    "{target} offers a trim/affix node but this test has no native \
                     strip marker for it — add one (or the renderer)"
                );
            }
            for (op, pat) in forms {
                let src = format!(
                    "v=\"  padded  \"\nx=${{v{op}{pat}}}\necho \"$x\"\n"
                );
                let mut p = crate::Parser::new(&src);
                let cmds = p.parse().expect("parse");
                let prog = crate::shir::ast_to_ir(&cmds);
                let out = render_for(target, &prog)
                    .unwrap_or_else(|| panic!("{target}: no renderer wired in this test"));
                assert!(
                    !out.contains("__sh_spawn("),
                    "{target}: ${{v{op}{pat}}} shells out:\n{out}"
                );
                assert!(
                    !out.contains("bash -c"),
                    "{target}: ${{v{op}{pat}}} spawns bash:\n{out}"
                );
                // The mis-parse's signature: the pattern's brackets glued
                // onto the operator inside a VARIABLE NAME (the slice bug
                // produced `v##*[[` + `space` + `]]`). A render that still
                // does this is looking up a variable that does not exist.
                for leak in ["v##[[", "v%%[[", "v#[[", "v%[["] {
                    assert!(
                        !out.contains(leak),
                        "{target}: ${{v{op}{pat}}} leaked the pattern into the variable name ({leak}):\n{out}"
                    );
                }
                // The glob-passing backends keep the PATTERN verbatim, so it
                // must arrive whole and not truncated at a bracket or a
                // colon. (perl converts it to a regex; estree lowers a
                // literal pattern to a delimiter — `#*:` becomes an
                // `indexOf(":")` slice, so the `*` is consumed by design.)
                if !NO_NATIVE_TRIM.contains(&target)
                    && !matches!(target, "perl" | "estree" | "js")
                {
                    assert!(
                        out.contains(pat),
                        "{target}: ${{v{op}{pat}}} dropped or mangled the pattern:\n{out}"
                    );
                }
                if !markers.is_empty() {
                    assert!(
                        markers.iter().any(|m| out.contains(m)),
                        "{target}: ${{v{op}{pat}}} took no native strip path (none of {markers:?}):\n{out}"
                    );
                } else {
                    // The documented gap: the param channel is a stub, so
                    // the program still PARSES to the right operator and
                    // never spawns — it just cannot compute it yet.
                    assert!(
                        out.contains("TODO(unsupported)"),
                        "{target}: ${{v{op}{pat}}} neither renders nor marks the gap:\n{out}"
                    );
                }
            }
        }
    }
}
