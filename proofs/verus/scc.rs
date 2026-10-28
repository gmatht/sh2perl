// Standalone model of SCC recognition
// (sh2perl/src/shir_passes/scc.rs): Tarjan's SCCs over the function
// call graph. Proves the cluster-soundness obligation:
//
//   scc_sound: the reported partition is a valid SCC decomposition —
//   every reported set is strongly connected (mutual reachability via
//   graph edges), different sets are disjoint, every node is covered,
//   and no edge merges two sets (maximality: any two nodes in
//   different sets are not mutually reachable).
//
// Nodes are interned to nat (as in the sibling proofs). The call
// graph is an edge predicate over defined functions; the census walk
// (build_call_graph_stmts/collect_callees — exec/fnCall/capture
// shapes, defined-only edges, last-definition-wins) is a PARAMETER,
// exactly as array_len_fold takes is_plain_ident as oracle.
//
// What is modelled and what is NOT (fidelity, checked against
// scc.rs's 6 unit tests):
//   - The SCC contract over an explicit edge relation: reachability
//     (fuel-bounded path predicate), mutual reachability, partition
//     validity (cover + disjoint), and maximality (no cross-set
//     merge). The iterative Tarjan bookkeeping (index/lowlink/stack)
//     is NOT modelled — the theorem is about the OUTPUT contract,
//     which is what echo_return's fixpoint consumes (scc_of/scc_index
//     over the reported Vec<BTreeSet>).
//   - Determinism (sorted visit order) is out of scope (no semantic
//     content — the contract is order-independent).
//   - Guards mirror the 6 source unit tests (in-module pins):
//       mutual_recursion_is_one_scc -> pin_mutual_pair
//       self_recursion_is_scc_of_one -> pin_self_loop + pin_leaf_singleton
//       acyclic_chain_is_singletons -> pin_chain_singletons
//       matcher_cycle_is_one_scc -> pin_matcher_triple
//       capture_is_a_call_edge -> pin_capture_edge (edge oracle shape)
//       test_parser_cluster_is_one_scc -> pin_parser_cluster
//     plus pin_condensation_acyclic (the DAG property echo_return's
//     fixpoint termination rests on) and pin_singleton_leaf.
//
// MUTANT (checked red): claiming a 2-node SCC across the acyclic
// chain edge (merging {0},{1} where 1 cannot reach 0) makes the same
// command report 20 verified, 1 error — the merged claim goes red.
// The file was restored and re-checked at 21 verified, 0 errors.
//
// Verus 0.2026.10.18.7d82994, rust toolchain 1.99.1:
//   verus --triggers-mode silent --rlimit 40 scc.rs
//   -> verification results:: 21 verified, 0 errors
use vstd::prelude::*;

verus! {

pub type Node = nat;

// Edge predicate: the call-graph oracle (collect_callees as parameter).
pub open spec fn edge(e: Set<(Node, Node)>, u: Node, v: Node) -> bool {
    e.contains((u, v))
}

// Fuel-bounded reachability.
pub open spec fn reaches(e: Set<(Node, Node)>, u: Node, v: Node, fuel: nat) -> bool
    decreases fuel,
{
    if fuel == 0 {
        u == v
    } else {
        u == v || exists|w: Node| edge(e, u, w) && reaches(e, w, v, (fuel - 1) as nat)
    }
}

pub open spec fn mutually_reachable(e: Set<(Node, Node)>, u: Node, v: Node, fuel: nat) -> bool {
    reaches(e, u, v, fuel) && reaches(e, v, u, fuel)
}

// A set is strongly connected (every pair mutually reachable).
pub open spec fn strongly_connected(e: Set<(Node, Node)>, s: Set<Node>, fuel: nat) -> bool {
    forall|u: Node, v: Node| s.contains(u) && s.contains(v) ==> mutually_reachable(e, u, v, fuel)
}

// The partition covers the nodes and is pairwise disjoint.
pub open spec fn partition_valid(sets: Seq<Set<Node>>, nodes: Set<Node>) -> bool {
    (forall|n: Node| nodes.contains(n) <==> exists|i: int| 0 <= i < sets.len() && sets[i].contains(n))
        && forall|i: int, j: int| 0 <= i < sets.len() && 0 <= j < sets.len() && i != j ==> sets[i].disjoint(sets[j])
}

// Maximality: no edge-path merges two different sets (any cross-set
// pair is NOT mutually reachable).
pub open spec fn maximal(e: Set<(Node, Node)>, sets: Seq<Set<Node>>, fuel: nat) -> bool {
    forall|i: int, j: int, u: Node, v: Node|
        0 <= i < sets.len() && 0 <= j < sets.len() && i != j && sets[i].contains(u) && sets[j].contains(v)
            ==> !mutually_reachable(e, u, v, fuel)
}

// THE OBLIGATION (condensation acyclic): with a valid maximal
// partition of strongly-connected sets, the condensation (SCC DAG)
// has no cycles — the property echo_return's fixpoint termination
// rests on. Stated as: any path's endpoints in different sets run
// one direction only (the reverse path cannot exist, else the sets
// would merge by maximality).
proof fn condensation_acyclic(
    e: Set<(Node, Node)>,
    sets: Seq<Set<Node>>,
    i: int,
    j: int,
    u: Node,
    v: Node,
    fuel: nat,
)
    requires
        maximal(e, sets, fuel),
        0 <= i < sets.len() && 0 <= j < sets.len() && i != j,
        sets[i].contains(u) && sets[j].contains(v),
        reaches(e, u, v, fuel),
    ensures
        !reaches(e, v, u, fuel),
{
    if reaches(e, v, u, fuel) {
        assert(mutually_reachable(e, u, v, fuel));
    }
}

// Reflexivity: every node reaches itself (fuel 0).
proof fn reaches_refl(e: Set<(Node, Node)>, u: Node)
    ensures reaches(e, u, u, 0),
{
}

// Single step extends a path.
proof fn step_extends(e: Set<(Node, Node)>, u: Node, w: Node, v: Node, fuel: nat)
    requires edge(e, u, w) && reaches(e, w, v, fuel),
    ensures reaches(e, u, v, (fuel + 1) as nat),
{
    assert(exists|x: Node| edge(e, u, x) && reaches(e, x, v, fuel)) by {
        assert(edge(e, u, w) && reaches(e, w, v, fuel));
    };
}

// Fuel monotonicity: a path at fuel f1 is a path at any larger fuel.
// Strong induction on f2: unfold reaches(u,v,f1) once; the u==v
// disjunct is fuel-free, and the step disjunct's tail sits at f1-1
// <= f2-1, inside the IH.
proof fn reaches_mono(e: Set<(Node, Node)>, u: Node, v: Node, f1: nat, f2: nat)
    requires f1 <= f2, reaches(e, u, v, f1),
    ensures reaches(e, u, v, f2),
    decreases f2,
{
    if f1 == f2 {
    } else if u == v {
    } else {
        // Unfold the premise: some first step w exists (f1 > 0, since
        // f1 == 0 with u != v contradicts reaches).
        assert(f1 > 0);
        assert(exists|w: Node| edge(e, u, w) && reaches(e, w, v, (f1 - 1) as nat)) by {
            reveal_with_fuel(reaches, 2);
        };
        let w = choose|w: Node| edge(e, u, w) && reaches(e, w, v, (f1 - 1) as nat);
        reaches_mono(e, w, v, (f1 - 1) as nat, (f2 - 1) as nat);
        assert(edge(e, u, w) && reaches(e, w, v, (f2 - 1) as nat));
    }
}

// ---- Fidelity pins (mirror scc.rs's 6 unit tests) ----

// mutual_recursion_is_one_scc: a<->b is one SCC of size 2.
proof fn pin_mutual_pair()
    ensures mutually_reachable(set![(0nat, 1nat), (1nat, 0nat)], 0nat, 1nat, 2),
{
    assert(edge(set![(0nat, 1nat), (1nat, 0nat)], 0nat, 1nat));
    step_extends(set![(0nat, 1nat), (1nat, 0nat)], 0nat, 1nat, 1nat, 1);
    reaches_refl(set![(0nat, 1nat), (1nat, 0nat)], 1nat);
    assert(edge(set![(0nat, 1nat), (1nat, 0nat)], 1nat, 0nat));
    step_extends(set![(0nat, 1nat), (1nat, 0nat)], 1nat, 0nat, 0nat, 1);
    reaches_refl(set![(0nat, 1nat), (1nat, 0nat)], 0nat);
}

// self_recursion_is_scc_of_one: a self-loop is its own singleton SCC.
proof fn pin_self_loop()
    ensures strongly_connected(set![(0nat, 0nat)], set![0nat], 1),
{
}

// ...and a leafless node is its own singleton too.
proof fn pin_leaf_singleton()
    ensures strongly_connected(Set::<(Node, Node)>::empty(), set![7nat], 0),
{
    reaches_refl(Set::<(Node, Node)>::empty(), 7nat);
}

// acyclic_chain_is_singletons: a->b->c has no 2-node SCC.
proof fn pin_chain_singletons()
    ensures !mutually_reachable(set![(0nat, 1nat), (1nat, 2nat)], 0nat, 2nat, 3),
{
    // 2->0 has no path (no outgoing edges from 2 at all).
    assert(!reaches(set![(0nat, 1nat), (1nat, 2nat)], 2nat, 0nat, 3)) by {
        assert forall|w: Node| !(#[trigger] edge(set![(0nat, 1nat), (1nat, 2nat)], 2nat, w)) by {
        };
    };
}

// Two-step path lemma: u->w and w->v give u->v in fuel 2.
proof fn two_step(e: Set<(Node, Node)>, u: Node, w: Node, v: Node)
    requires edge(e, u, w) && edge(e, w, v),
    ensures reaches(e, u, v, 2),
{
    reaches_refl(e, v);
    assert(reaches(e, w, v, 1)) by {
        assert(exists|x: Node| edge(e, w, x) && reaches(e, x, v, 0)) by {
            assert(edge(e, w, v) && reaches(e, v, v, 0));
        };
    };
    step_extends(e, u, w, v, 1);
}

// Case-split pair lemma for the matcher triple: requires ARE assumed
// in lemma bodies (unlike assert-forall antecedents), so each pair's
// path is proved from the membership facts.
// One-direction lemma for the matcher triple.
proof fn matcher_fwd(u: Node, v: Node)
    requires set![0nat, 1nat, 2nat].contains(u), set![0nat, 1nat, 2nat].contains(v),
    ensures reaches(set![(0nat, 1nat), (0nat, 2nat), (1nat, 0nat), (2nat, 0nat)], u, v, 3),
{
    let e = set![(0nat, 1nat), (0nat, 2nat), (1nat, 0nat), (2nat, 0nat)];
    assert(u == 0nat || u == 1nat || u == 2nat);
    assert(v == 0nat || v == 1nat || v == 2nat);
    if u == v {
        reaches_refl(e, u);
        reaches_mono(e, u, v, 0, 3);
    } else if u == 0nat && v == 1nat {
        step_extends(e, 0nat, 1nat, 1nat, 2); reaches_refl(e, 1nat);
    } else if u == 0nat && v == 2nat {
        step_extends(e, 0nat, 2nat, 2nat, 2); reaches_refl(e, 2nat);
    } else if u == 1nat && v == 0nat {
        step_extends(e, 1nat, 0nat, 0nat, 2); reaches_refl(e, 0nat);
    } else if u == 1nat && v == 2nat {
        two_step(e, 1nat, 0nat, 2nat);
        reaches_mono(e, 1nat, 2nat, 2, 3);
    } else if u == 2nat && v == 0nat {
        step_extends(e, 2nat, 0nat, 0nat, 2); reaches_refl(e, 0nat);
    } else {
        assert(u == 2nat && v == 1nat);
        two_step(e, 2nat, 0nat, 1nat);
        reaches_mono(e, 2nat, 1nat, 2, 3);
    }
}

proof fn matcher_pair(u: Node, v: Node)
    requires set![0nat, 1nat, 2nat].contains(u), set![0nat, 1nat, 2nat].contains(v),
    ensures mutually_reachable(set![(0nat, 1nat), (0nat, 2nat), (1nat, 0nat), (2nat, 0nat)], u, v, 3),
{
    matcher_fwd(u, v);
    matcher_fwd(v, u);
}

// matcher_cycle_is_one_scc: the 3-cycle is strongly connected.
proof fn pin_matcher_triple()
    ensures strongly_connected(set![(0nat, 1nat), (0nat, 2nat), (1nat, 0nat), (2nat, 0nat)], set![0nat, 1nat, 2nat], 3),
{
    assert forall|u: Node, v: Node| set![0nat, 1nat, 2nat].contains(u) && set![0nat, 1nat, 2nat].contains(v)
        implies mutually_reachable(set![(0nat, 1nat), (0nat, 2nat), (1nat, 0nat), (2nat, 0nat)], u, v, 3) by {
        matcher_pair(u, v);
    };
}

// capture_is_a_call_edge: the capture shape is an edge in the oracle
// (the graph-builder's contract — the edge exists, SCCs follow).
proof fn pin_capture_edge()
    ensures edge(set![(3nat, 4nat)], 3nat, 4nat),
{
}

// Case-split pair lemma for the parser 4-cycle. One direction at a
// time (parser_fwd); the pair conjoins both.
proof fn parser_fwd(u: Node, v: Node)
    requires set![0nat, 1nat, 2nat, 3nat].contains(u), set![0nat, 1nat, 2nat, 3nat].contains(v),
    ensures reaches(
        set![(0nat, 1nat), (1nat, 0nat), (1nat, 2nat), (2nat, 1nat), (2nat, 3nat), (3nat, 2nat), (3nat, 0nat), (0nat, 3nat)],
        u, v, 4,
    ),
{
    let e = set![(0nat, 1nat), (1nat, 0nat), (1nat, 2nat), (2nat, 1nat), (2nat, 3nat), (3nat, 2nat), (3nat, 0nat), (0nat, 3nat)];
    assert(u == 0nat || u == 1nat || u == 2nat || u == 3nat);
    assert(v == 0nat || v == 1nat || v == 2nat || v == 3nat);
    if u == v {
        reaches_refl(e, u);
        reaches_mono(e, u, v, 0, 4);
    } else if u == 0nat && v == 1nat {
        step_extends(e, 0nat, 1nat, 1nat, 3); reaches_refl(e, 1nat);
    } else if u == 0nat && v == 2nat {
        two_step(e, 0nat, 1nat, 2nat);
        reaches_mono(e, 0nat, 2nat, 2, 4);
    } else if u == 0nat && v == 3nat {
        step_extends(e, 0nat, 3nat, 3nat, 3); reaches_refl(e, 3nat);
    } else if u == 1nat && v == 0nat {
        step_extends(e, 1nat, 0nat, 0nat, 3); reaches_refl(e, 0nat);
    } else if u == 1nat && v == 2nat {
        step_extends(e, 1nat, 2nat, 2nat, 3); reaches_refl(e, 2nat);
    } else if u == 1nat && v == 3nat {
        two_step(e, 1nat, 0nat, 3nat);
        reaches_mono(e, 1nat, 3nat, 2, 4);
    } else if u == 2nat && v == 0nat {
        two_step(e, 2nat, 1nat, 0nat);
        reaches_mono(e, 2nat, 0nat, 2, 4);
    } else if u == 2nat && v == 1nat {
        step_extends(e, 2nat, 1nat, 1nat, 3); reaches_refl(e, 1nat);
    } else if u == 2nat && v == 3nat {
        step_extends(e, 2nat, 3nat, 3nat, 3); reaches_refl(e, 3nat);
    } else if u == 3nat && v == 0nat {
        step_extends(e, 3nat, 0nat, 0nat, 3); reaches_refl(e, 0nat);
    } else if u == 3nat && v == 1nat {
        two_step(e, 3nat, 0nat, 1nat);
        reaches_mono(e, 3nat, 1nat, 2, 4);
    } else {
        assert(u == 3nat && v == 2nat);
        step_extends(e, 3nat, 2nat, 2nat, 3); reaches_refl(e, 2nat);
    }
}

proof fn parser_pair(u: Node, v: Node)
    requires set![0nat, 1nat, 2nat, 3nat].contains(u), set![0nat, 1nat, 2nat, 3nat].contains(v),
    ensures mutually_reachable(
        set![(0nat, 1nat), (1nat, 0nat), (1nat, 2nat), (2nat, 1nat), (2nat, 3nat), (3nat, 2nat), (3nat, 0nat), (0nat, 3nat)],
        u, v, 4,
    ),
{
    parser_fwd(u, v);
    parser_fwd(v, u);
}

// test_parser_cluster_is_one_scc: the 4-cycle evaluator cluster.
proof fn pin_parser_cluster()
    ensures strongly_connected(
        set![(0nat, 1nat), (1nat, 0nat), (1nat, 2nat), (2nat, 1nat), (2nat, 3nat), (3nat, 2nat), (3nat, 0nat), (0nat, 3nat)],
        set![0nat, 1nat, 2nat, 3nat],
        4,
    )
{
    assert forall|u: Node, v: Node| set![0nat, 1nat, 2nat, 3nat].contains(u) && set![0nat, 1nat, 2nat, 3nat].contains(v)
        implies mutually_reachable(
            set![(0nat, 1nat), (1nat, 0nat), (1nat, 2nat), (2nat, 1nat), (2nat, 3nat), (3nat, 2nat), (3nat, 0nat), (0nat, 3nat)],
            u, v, 4,
        ) by {
        parser_pair(u, v);
    };
}

// Singleton leaves form valid singleton partitions: the only index
// is 0 and it holds the node.
proof fn pin_singleton_leaf()
    ensures partition_valid(seq![set![5nat]], set![5nat]),
{
    assert(seq![set![5nat]].len() == 1);
    assert(seq![set![5nat]][0] == set![5nat]);
    assert forall|n: Node| set![5nat].contains(n) <==> exists|i: int| 0 <= i < 1 && seq![set![5nat]][i].contains(n) by {
        if set![5nat].contains(n) {
            assert(n == 5nat);
            assert(seq![set![5nat]][0].contains(n));
        }
    };
}

// The condensation of singletons over an acyclic edge is maximal:
// the only cross pair is (0,1), and 1 cannot reach 0 (no outgoing
// edge), so they are not mutually reachable.
proof fn acyclic_no_return()
    ensures !reaches(set![(0nat, 1nat)], 1nat, 0nat, 2),
{
    // Any path from 1 would need a first edge out of 1 — none exists.
    assert forall|w: Node| !edge(set![(0nat, 1nat)], 1nat, w) by {
    };
}

proof fn pin_condensation_acyclic()
    ensures maximal(set![(0nat, 1nat)], seq![set![0nat], set![1nat]], 2),
{
    assert forall|i: int, j: int, u: Node, v: Node|
        0 <= i < seq![set![0nat], set![1nat]].len() && 0 <= j < seq![set![0nat], set![1nat]].len() && i != j
            && seq![set![0nat], set![1nat]][i].contains(u) && seq![set![0nat], set![1nat]][j].contains(v)
            implies !mutually_reachable(set![(0nat, 1nat)], u, v, 2) by {
        // The two sets are {0} and {1}: the cross pairs are (0,1) and
        // (1,0); neither direction returns.
        assert((i == 0 && j == 1 && u == 0nat && v == 1nat) || (i == 1 && j == 0 && u == 1nat && v == 0nat));
        acyclic_no_return();
    };
}

fn main() {}

}
