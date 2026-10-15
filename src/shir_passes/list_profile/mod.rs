//! List-container profiling (docs/LISTS.MD §4) — analyses only.
//!
//! Census, frequency bounds, and structure selection per list variable.
//! The C backend consumes the verdicts for growable-vec selection
//! (`Fixed` keeps fixed storage; the rest lower as Vec until their own
//! lowerings land). No other pipeline stage or backend consumes them.
//!
//! Provenance: prototyped out-of-tree in `scratch-listprof/` (14 unit
//! tests + 90/90 py-sh-go A1 profiles, no crashes) and moved here
//! near-verbatim. Known prototype gaps are documented on each item:
//! global (unscoped) names, `setArray`-in-loop reset over-approximation,
//! untracked return-value flow, advisory-only loop depths.

pub mod bounds;
pub mod census;
pub mod scopes;
pub mod select;

pub use scopes::{Scope, Use, local_owner, local_scope, use_scopes};

pub use bounds::{TripCtx, append_hi, has_unbounded_site, max_site_depth, trip_of_loop};
pub use census::{Census, ListInfo, Op, Site, SiteKind, census};
pub use select::{Structure, analyze};

use std::collections::BTreeMap;

use crate::ir::IrProgram;

/// Structured per-list verdict for backend consumers (prefer over the
/// JSON [`analyze`] report when rendering: no serialization round-trip).
#[derive(Clone, Debug)]
pub struct ListVerdict {
    pub structure: Structure,
    pub append_hi: Option<u64>,
    pub escaped: bool,
}

/// Full analysis → structured verdicts by list name.
pub fn verdicts(prog: &IrProgram) -> BTreeMap<String, ListVerdict> {
    let c = census(prog);
    c.lists
        .iter()
        .map(|(name, info)| {
            let (structure, _) = select::select(info);
            (
                name.clone(),
                ListVerdict {
                    structure,
                    append_hi: append_hi(info),
                    escaped: info.escaped,
                },
            )
        })
        .collect()
}
