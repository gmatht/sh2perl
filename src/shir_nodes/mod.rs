//! shir_nodes — transform-declared IR nodes: the union + their JSON.
//!
//! PLUGGABLE_NESTED_TRANSFORMS.md §2. A transform that needs a new A1 node
//! declares it once in `shir_nodes/*.node` (name, JSON tag, fields);
//! `build.rs` parses the declaration and generates, into OUT_DIR:
//!   - the Rust `struct` for the node,
//!   - `to_json` / `from_json` (the JSON generator AND parser, from one
//!     source — they cannot drift),
//!   - the `children_mut` child accessor (tree-walking reachability),
//!   - the registry `all_nodes()` — the union of every transform-declared
//!     node (tag → constructor).
//!
//! The generated nodes are NOT yet spliced into the closed `IrStmt` enum
//! (that migration — an `Ext(Box<dyn ExtNode>)` slot, the derive fallout,
//! and wiring the core hand-written `shir_json_in`/`shir_json` matchers —
//! is the documented follow-on). This module proves the mechanism end to
//! end: JSON round-trip (to_json → from_json → equal) and union lookup by
//! tag.

use crate::ir::IrStmt;

pub mod enc;

/// A transform-declared node. The `tag` is the JSON discriminator
/// (`{"type": <tag>, …}`); `children_mut` lets pre-existing walkers descend
/// into a node they don't understand (structural traversal, §1 of the doc).
pub trait ExtNode: std::fmt::Debug {
    fn tag(&self) -> &'static str;
    fn to_json(&self) -> serde_json::Value;
    fn children_mut(&mut self) -> Vec<&mut IrStmt>;
    /// Immutable children (for the analysis passes that recurse on `&IrStmt`).
    fn children(&self) -> Vec<&IrStmt>;
    /// Clone-erasure so `Box<dyn ExtNode>` can be `Clone` (needed by the
    /// `IrStmt::Ext` variant's `#[derive(Clone)]`).
    fn clone_box(&self) -> Box<dyn ExtNode>;
}

// The `IrStmt` enum derives Debug/Clone/PartialEq; the `Ext(Box<dyn
// ExtNode>)` variant needs those on `Box<dyn ExtNode>`. Rust's blanket
// `impl Debug for Box<T>` already covers us (ExtNode: Debug, and the
// generated structs derive Debug); implement Clone (via clone_box — the
// blanket needs `dyn ExtNode: Clone`, impossible) and PartialEq (compare
// tag + JSON — semantic equality for generated nodes).
impl Clone for Box<dyn ExtNode> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
impl PartialEq for Box<dyn ExtNode> {
    fn eq(&self, other: &Self) -> bool {
        self.tag() == other.tag() && self.to_json() == other.to_json()
    }
}

/// Registry constructor: (JSON) → boxed node.
pub(crate) type NodeCtor = fn(&serde_json::Value) -> Result<Box<dyn ExtNode>, String>;

// The generated structs + `all_nodes()` registry (build.rs → OUT_DIR).
include!(concat!(env!("OUT_DIR"), "/shir_nodes_gen.rs"));

/// Union lookup: find the constructor for a node tag.
pub(crate) fn node_ctor(tag: &str) -> Option<NodeCtor> {
    all_nodes()
        .into_iter()
        .find(|(t, _)| *t == tag)
        .map(|(_, ctor)| ctor)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mechanism end to end: build the declared node, emit JSON, parse
    /// it back — byte-identical JSON and an equal node.
    #[test]
    fn declared_node_json_round_trip() {
        let node = CountedFor {
            var: "i".to_string(),
            init: 0,
            step: 1,
            cond: crate::ir::IrExpr::Int(10),
            body: vec![IrStmt::Expr(crate::ir::IrExpr::Int(1))],
        };
        let json = node.to_json();
        let parsed = CountedFor::from_json(&json).expect("parse");
        assert_eq!(parsed, node, "from_json(to_json(x)) == x");
        // and the JSON is stable (determinism for the contract)
        assert_eq!(node.to_json().to_string(), json.to_string());
    }

    /// The union: a registered tag resolves to a constructor that yields the
    /// same node shape back out.
    #[test]
    fn union_lookup_by_tag() {
        let ctor = node_ctor("CountedFor").expect("declared node in the union");
        let json = serde_json::json!({
            "type": "CountedFor",
            "var": "j",
            "init": 3,
            "step": 2,
            "cond": {"kind": "Int", "value": 7},
            "body": [],
        });
        let node = ctor(&json).expect("construct");
        assert_eq!(node.tag(), "CountedFor");
        assert_eq!(node.to_json()["var"], "j");
        // every tag in the registry is present
        assert!(all_nodes().iter().any(|(t, _)| *t == "CountedFor"));
    }

    /// The generated child accessor reaches the node's nested statements
    /// (tree-walking reachability for transforms that don't know the node).
    #[test]
    fn declared_node_children_reachable() {
        let mut node = CountedFor {
            var: "k".to_string(),
            init: 0,
            step: 1,
            cond: crate::ir::IrExpr::Int(5),
            body: vec![IrStmt::Expr(crate::ir::IrExpr::Int(9))],
        };
        let children = node.children_mut();
        assert_eq!(children.len(), 1, "the body Vec<IrStmt> is reachable");
    }

    /// The full enum wiring: an `IrStmt::Ext` node survives the A1
    /// round-trip (export via stmt_json → the node's own JSON; import via
    /// the generated union registry → an IrStmt::Ext again).
    #[test]
    fn ext_node_round_trips_through_a1() {
        use crate::ir::IrProgram;
        let ext = IrStmt::Ext(Box::new(CountedFor {
            var: "i".to_string(),
            init: 0,
            step: 1,
            cond: crate::ir::IrExpr::Int(10),
            body: vec![IrStmt::Expr(crate::ir::IrExpr::Int(1))],
        }));
        let prog = IrProgram {
            imports: vec![],
            requires: vec![],
            stmts: vec![ext],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
            var_nospace: vec![],
            var_bash_env: vec![],
        };
        let json = crate::shir_json::shir_to_shir_json(&prog);
        let back = crate::shir_json_in::shir_json_to_ir(&json).expect("re-import");
        let back_ext = back.stmts.iter().find_map(|s| match s {
            IrStmt::Ext(n) => Some(crate::shir_nodes::ExtNode::to_json(n.as_ref())),
            _ => None,
        });
        assert!(
            back_ext.is_some(),
            "Ext node survived the A1 round-trip: {json}"
        );
        assert_eq!(back_ext.unwrap()["var"], "i");
    }
}
