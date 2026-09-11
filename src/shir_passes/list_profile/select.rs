//! Structure selection (LISTS.MD §4.4) — prototype.
//!
//! Maps a list's census + bounds to a container structure with reasons.
//! The class-split dimension (int vs bigint extents) belongs to
//! OPTIMISED_LIST.md and is orthogonal: every structure below stores
//! *extents* when the list is class-mixed.

use std::collections::BTreeMap;

use crate::ir::IrProgram;

use super::bounds::{append_hi, has_unbounded_site, max_site_depth};
use super::census::{ListInfo, Op, census};

/// Candidate container shapes (LISTS.MD §2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Structure {
    /// Fixed static array + len (status quo): zero allocation, hard cap.
    Fixed,
    /// Growable dynamic array (doubling realloc).
    Vec,
    /// Inline-N + heap spill (unbounded but usually small).
    SmallVec,
    /// Chunked deque of blocks (double-ended / scattered mutation).
    Chunked,
    /// Gap buffer at one fixed cursor (localized mutation).
    Gap,
    /// Concat tree (concat-dominated).
    Rope,
}

impl Structure {
    fn as_str(self) -> &'static str {
        match self {
            Structure::Fixed => "Fixed",
            Structure::Vec => "Vec",
            Structure::SmallVec => "SmallVec",
            Structure::Chunked => "Chunked",
            Structure::Gap => "Gap",
            Structure::Rope => "Rope",
        }
    }
}

/// C-backend `ARR_CAP` (fixed-array capacity).
pub const ARR_CAP: u64 = 1024;
/// Small-vector inline threshold (tunable per §6.4; prototype default).
pub const INLINE_N: u64 = 16;

/// Select a structure for one list. Returns the choice plus human-readable
/// reasons (the report carries them so selections stay auditable).
pub fn select(info: &ListInfo) -> (Structure, Vec<String>) {
    let mut why = vec![];
    let midmut = !info.midmut_sites.is_empty();
    let concat = !info.concat_sites.is_empty();
    let depth = max_site_depth(info);
    let unbounded = has_unbounded_site(info);

    if info.escaped || info.ops.contains(&Op::Escape) {
        why.push("escapes to unknown code: keep status-quo fixed array".to_string());
        return (Structure::Fixed, why);
    }
    if midmut {
        if let Some(c) = info.fixed_cursor {
            why.push(format!("all middle mutation at constant index {c}: gap buffer"));
            return (Structure::Gap, why);
        }
        if unbounded || depth >= 1 {
            why.push("unbounded or loop-nested middle mutation: chunked deque".to_string());
            return (Structure::Chunked, why);
        }
        why.push("bounded straight-line middle mutation only".to_string());
        // fall through to the capacity rule below (memmove priced OK)
    }
    if concat && depth >= 1 {
        why.push("loop-nested concat: rope".to_string());
        return (Structure::Rope, why);
    }
    match append_hi(info) {
        Some(h) if h <= ARR_CAP && !midmut => {
            why.push(format!("append upper bound {h} ≤ cap {ARR_CAP}: fixed array"));
            return (Structure::Fixed, why);
        }
        Some(h) if h <= ARR_CAP => {
            why.push(format!(
                "append upper bound {h} ≤ cap but straight-line middle mutation: vec (memmove priced OK)"
            ));
            return (Structure::Vec, why);
        }
        Some(h) => {
            why.push(format!("append upper bound {h} exceeds cap: growable vec"));
            return (Structure::Vec, why);
        }
        None => {}
    }
    // Unbounded appends: inline fast path only when usually small.
    if !midmut && info.literal_floor <= INLINE_N {
        why.push(format!(
            "unbounded appends but floor {} ≤ inline {INLINE_N}: small-vector",
            info.literal_floor
        ));
        return (Structure::SmallVec, why);
    }
    why.push("unbounded appends: growable vec".to_string());
    (Structure::Vec, why)
}

/// Op names for the report (stable strings).
fn op_name(o: Op) -> &'static str {
    match o {
        Op::Def => "def",
        Op::Append => "append",
        Op::IndexRead => "index-read",
        Op::IndexWrite => "index-write",
        Op::Slice => "slice",
        Op::Len => "len",
        Op::Iterate => "iterate",
        Op::Read => "read",
        Op::Sorted => "sorted",
        Op::Max => "max",
        Op::Min => "min",
        Op::Sum => "sum",
        Op::Concat => "concat",
        Op::MidMut => "midmut",
        Op::EndPop => "endpop",
        Op::Escape => "escape",
    }
}

/// Full analysis → JSON report value.
pub fn analyze(prog: &IrProgram) -> serde_json::Value {
    let c = census(prog);
    let mut lists = serde_json::Map::new();
    // deterministic order via BTreeMap iteration
    let ordered: BTreeMap<_, _> = c.lists.into_iter().collect();
    for (name, info) in &ordered {
        let (structure, why) = select(info);
        let ops: Vec<_> = info.ops.iter().map(|o| op_name(*o).to_string()).collect();
        lists.insert(
            name.clone(),
            serde_json::json!({
                "ops": ops,
                "append_hi": append_hi(info),
                "floor": info.literal_floor,
                "maxdepth": max_site_depth(info),
                "escaped": info.escaped,
                "fixed_cursor": info.fixed_cursor,
                "dyn_index_write": info.dyn_index_write,
                "structure": structure.as_str(),
                "why": why,
            }),
        );
    }
    serde_json::json!({
        "lists": lists,
        "dynamic_poison": c.dynamic_poison,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shir_passes::list_profile::census::{Site, SiteKind};

    fn info_with(ops: &[Op], append_hi_sites: Vec<Site>, floor: u64) -> ListInfo {
        ListInfo {
            ops: ops.iter().cloned().collect(),
            literal_floor: floor,
            append_sites: append_hi_sites,
            midmut_sites: vec![],
            concat_sites: vec![],
            fixed_cursor: None,
            max_const_index: None,
            dyn_index_write: false,
            escaped: false,
        }
    }

    fn site(count: u64, trip: Option<u64>, depth: usize) -> Site {
        Site { kind: SiteKind::Append, count, trip, depth, idx: None, fun: None }
    }

    #[test]
    fn bounded_append_only_stays_fixed() {
        let info = info_with(&[Op::Def, Op::Append, Op::Sorted], vec![site(3, Some(1), 0)], 3);
        // append_hi = 3 + 3 = 6 ≤ cap
        let (s, _) = select(&info);
        assert_eq!(s, Structure::Fixed);
    }

    #[test]
    fn overflow_bound_selects_vec() {
        let info = info_with(&[Op::Def, Op::Append], vec![site(1, Some(2000), 1)], 0);
        let (s, _) = select(&info);
        assert_eq!(s, Structure::Vec);
    }

    #[test]
    fn unbounded_small_selects_smallvec() {
        let info = info_with(&[Op::Def, Op::Append], vec![site(1, None, 1)], 2);
        let (s, _) = select(&info);
        assert_eq!(s, Structure::SmallVec);
    }

    #[test]
    fn unbounded_large_selects_vec() {
        let info = info_with(&[Op::Def, Op::Append], vec![site(1, None, 1)], 100);
        let (s, _) = select(&info);
        assert_eq!(s, Structure::Vec);
    }

    #[test]
    fn escape_selects_fixed() {
        let mut info = info_with(&[Op::Def, Op::Append], vec![site(1, None, 1)], 0);
        info.escaped = true;
        let (s, _) = select(&info);
        assert_eq!(s, Structure::Fixed);
    }

    #[test]
    fn fixed_cursor_selects_gap() {
        let mut info = info_with(&[Op::Def, Op::MidMut], vec![], 0);
        info.midmut_sites.push(Site {
            kind: SiteKind::MidMut,
            count: 1,
            trip: Some(5),
            depth: 1,
            idx: Some(0),
            fun: None,
        });
        info.fixed_cursor = Some(0);
        let (s, _) = select(&info);
        assert_eq!(s, Structure::Gap);
    }

    #[test]
    fn scattered_midmut_selects_chunked() {
        let mut info = info_with(&[Op::Def, Op::MidMut], vec![], 0);
        info.midmut_sites.push(Site {
            kind: SiteKind::MidMut,
            count: 1,
            trip: None,
            depth: 1,
            idx: None,
            fun: None,
        });
        let (s, _) = select(&info);
        assert_eq!(s, Structure::Chunked);
    }

    #[test]
    fn loop_concat_selects_rope() {
        let mut info = info_with(&[Op::Def], vec![], 0);
        info.concat_sites.push(Site {
            kind: SiteKind::Concat,
            count: 1,
            trip: Some(10),
            depth: 1,
            idx: None,
            fun: None,
        });
        info.ops.insert(Op::Concat);
        let (s, _) = select(&info);
        assert_eq!(s, Structure::Rope);
    }
}
