// Standalone model of the C backend's GROWTH-HELPER FAMILY — the
// emitted runtime's dynamic-buffer protocol that two gated models
// listed as an ASSUMPTION ("vbuf growth assumed"): C_UB_PROOF.md §20
// (c_str_helpers) and §21 (c_capture_wrap) both discharge their
// capacity claims to it. Every helper below is transcribed in shape
// from sh2perl/src/c_backend.rs:
//
//   A — `_sh_grow(char **b, size_t *cap, size_t need)` (≈11604):
//         if (need <= *cap) return;
//         *cap = need * 2;
//         np = realloc(*b, *cap); if (!np) { fprintf; exit(127); }
//         *b = np;
//   B — `_sh_vgrow(_sh_vbuf *b, size_t need)` (≈11619), with
//         `typedef struct { char *p; size_t cap; } _sh_vbuf`:
//         if (need <= b->cap && b->p) return b->p;
//         ncap = b->cap ? b->cap : 256;
//         while (ncap < need) ncap *= 2;
//         np = realloc(b->p, ncap); if (!np) { fprintf; exit(127); }
//         b->p = np; b->cap = ncap; return np;
//   C — `_sh_grow_str(char ***, size_t *cap)` (≈11925) and the typed
//         `_sh_grow_{i8,u8,i16,u16,i32,u32,u64,ll}` (≈11939):
//         ncap = *cap ? *cap * 2 : 16;
//         realloc(*p, ncap * sizeof *np);  the str variant then
//         ZERO-FILLS [ocap, ncap) — "an uninitialized slot would
//         free garbage" (the source's own comment).
//   D — `_sh_reserve_{str,t}(p, cap, need)` (≈11957): exact reserve
//         WITH the guard `need > (size_t)-1 / sizeof …` before the
//         multiply — the ONE member of the family whose size
//         arithmetic is guarded (the asymmetry worth proving: D's
//         theorem needs no domain proviso, A/B/C's do).
//
//   Consumers modelled: `_sh_add` / `_sh_addc` (≈11630), `_sh_fa`
//   (ECHO_FAST_C ≈9126), the `snprintf_grow` loop (≈10335, the
//   grow-until-sprintf-fits idiom behind every vbuf temp), the
//   vec-slot guard `while (len+1 > cap) _sh_grow_str(&id, &id_cap)`
//   (≈32040), and `_sh_fgets_line`'s capacity/NUL discipline
//   (≈12553; its loop-termination rests on fgets' file-bounded
//   progress, stated honestly, not proved here).
//
// THE SEMANTICS EXTENSION (the "extend as convenient" half): size_t
// arithmetic made WRAP-EXPLICIT — `u64(x) = x mod 2^64` — so every
// capacity theorem carries its domain EXACTLY instead of assuming
// arithmetic C does not give us. The thresholds are then measured,
// not vague: `_sh_grow`'s `need * 2` wraps at need = 2^63 (cap := 0
// while the caller writes at need > cap — pinned below), and
// `_sh_vgrow`'s doubling loop stalls at 2^63 with no progress past
// it (the raw loop wraps to 0 and spins — pinned in fuel form).
// Reaching those thresholds requires an exabyte-class string, so the
// domain provisos are honest-but-unreachable, exactly like
// c_width_store's range provisos.
//
// WHAT IS PROVED:
//   - family core: fast-path identity, `cap >= need` after every
//     growth (the postcondition EVERY consumer derefs), state
//     well-formedness preserved — alloc ⇔ cap ≥ 1 ⇔ p non-null, the
//     deref-safety `_sh_vgrow`'s `&& b->p` exists for — and the two
//     exact wrap thresholds as pins;
//   - `_sh_add`/`_sh_addc`/`_sh_fa`: byte-exact append, terminator
//     position, in-bounds write (the `+1` in the need — mutant M1
//     drops it), and the two append emissions agreeing;
//   - `_sh_vgrow`: total non-null return, doubling lands ≥ need
//     under `need ≤ 2^63`, and the amortization bound
//     `cap ≤ max(base, 2·need)`;
//   - `snprintf_grow`: room on the initial grow (size ≥ 256, p
//     non-null), per-iteration progress (cap strictly increases),
//     the break condition yielding the EXACT formatted value in
//     bounds, and termination over ANY int-typed return sequence —
//     within `imax - cap + 2` formatter calls the loop must break,
//     because growths push cap past imax and int returns are capped;
//   - `_sh_grow_str`: exact doubling cap, byte-size fit, old slots
//     preserved, fresh region entirely NULL (mutant M2 drops the
//     zero-fill); the vec-slot guard terminates and the following
//     `id[len++]` store lands in bounds;
//   - `_sh_reserve`: the guard fires on oversized requests, and a
//     request that passes multiplies EXACTLY — no wrap, no proviso
//     (mutant M3 drops the guard);
//   - `_sh_fgets_line`: after `vgrow(n+512)` the fgets size is
//     positive and its NUL lands at cap-1 (in bounds), and the
//     final `vgrow(n+1); p[n] = 0` lands in bounds.
//
// Honest (in file): the allocator contract — a successful realloc
// preserves bytes — is the premise here in the role `system`/`popen`
// play for §21; OOM exits 127 (an abort: defined behaviour, outside
// every theorem — which is also why `_sh_grow` writing *cap BEFORE
// the realloc check is harmless: failure exits before any consumer
// runs). The formatter's VALUE is the oracle (its spelling is
// §14b/§23's business); only its return/length contract is used, and
// encoding error (snprintf < 0, unspecified contents in C) is
// modelled break-with-unchanged-content — the emitted formats are
// %s/%d-class and cannot fail. Typed-grow slots beyond ocap stay
// UNINITIALIZED by design (no zero-fill): reads are len-guarded by
// the render layer (c_backend.rs:11921 comment) — that guard is the
// render-discipline layer's business, not modelled here. The
// `_sh_fgets_line` loop's own termination (fgets progress over a
// finite file) is stated, not proved.
//
// Negative controls (in-file mutants): M1 `sh_add_mut` drops the
// `+1` from the need (boundary: cap exactly l+n → fast path → the
// NUL lands one past the allocation); M2 `grow_str_mut` deletes the
// zero-fill; M3 `mutant_reserve_unguarded` drops the guard
// (2^61 × 8 wraps to 0). Source-level tie: the C gate and
// `harness/c_gate_ubsan.sh` — the family is what those sanitizer
// sweeps run under.
use vstd::prelude::*;

verus! {

// ── The machine: size_t arithmetic, wrap-explicit ────────────────────

pub open spec fn SZ() -> int {
    0x1_0000_0000_0000_0000int
}

// The C size_t computation `x`, for x >= 0: wrapping at 2^64.
pub open spec fn u64(x: int) -> int {
    x % SZ()
}

// `int`'s upper bound — snprintf returns int, so every formatter
// return the loop sees is <= this.
pub open spec fn imax() -> int {
    0x7FFF_FFFFint
}

// ── Buffer state ─────────────────────────────────────────────────────

// A live char buffer. `content` is the bytes BEFORE the terminator;
// `alloc` mirrors `p != NULL`. The emitted runtime maintains these
// on every observable state: buffers start `{0,0}`, each growth path
// only publishes cap through a successful realloc, and failure exits
// (see header) — so a NULL pointer never carries content.
pub struct Buf {
    pub content: Seq<char>,
    pub cap: int,
    pub alloc: bool,
}

pub open spec fn wf(b: Buf) -> bool {
    b.alloc == (b.cap >= 1) && 0 <= b.cap < SZ() && (b.alloc ==> b.content.len() + 1 <= b.cap)
        && (!b.alloc ==> b.content.len() == 0)
}

// The callers' domain proviso (honest — see header: reaching it
// needs an exabyte-class string).
pub open spec fn fits2(need: int) -> bool {
    2 * need < SZ()
}

// ── A: `_sh_grow` ────────────────────────────────────────────────────

// Total under the allocator premise (successful realloc preserves
// bytes; OOM exits — see header). The wrap in `need * 2` is modelled,
// not assumed away: outside `fits2` the capacity postcondition is
// simply false (the pin below exhibits exactly where).
pub open spec fn sh_grow(b: Buf, need: int) -> Buf {
    if need <= b.cap {
        b
    } else {
        Buf {
            content: b.content,
            cap: u64(2 * need),
            alloc: true,
        }
    }
}

// Fast path is identity — `if (need <= *cap) return;`.
pub proof fn grow_fastpath(b: Buf, need: int)
    ensures
        need <= b.cap ==> sh_grow(b, need) == b,
{
    if need <= b.cap {
        assert(sh_grow(b, need) == b);
    }
}

// THE capacity postcondition consumers deref: after `_sh_grow`,
// cap >= need, and the state stays well formed.
pub proof fn grow_room(b: Buf, need: int)
    requires
        wf(b),
        0 <= need,
        fits2(need),
    ensures
        sh_grow(b, need).cap >= need,
        wf(sh_grow(b, need)),
{
    if need <= b.cap {
        assert(sh_grow(b, need).cap == b.cap);
    } else {
        assert(u64(2 * need) == 2 * need);
        assert(2 * need >= need);
        assert(wf(sh_grow(b, need)));
    }
}

// The pinned threshold: at need = 2^63 the source's `*cap = need * 2`
// wraps to 0 while the caller still writes at need > 0 = cap — the
// exact state where the fast path would later "fit" an oversized
// write. Unreachable (exabyte string); load-bearing domain.
pub proof fn pin_grow_wrap_threshold()
    ensures
        sh_grow(Buf { content: Seq::empty(), cap: 0, alloc: false }, 0x8000_0000_0000_0000int).cap
            == 0,
        !wf(
            sh_grow(Buf { content: Seq::empty(), cap: 0, alloc: false }, 0x8000_0000_0000_0000int),
        ),
{
    let r = sh_grow(Buf { content: Seq::empty(), cap: 0, alloc: false }, 0x8000_0000_0000_0000int);
    assert(r.cap == u64(0x1_0000_0000_0000_0000int));
    assert(r.cap == 0);
    assert(!wf(r));
}

// ── Consumers of A: `_sh_add`, `_sh_addc`, `_sh_fa` ─────────────────

// `_sh_add`: grow to l+n+1, memcpy n+1 (s PLUS its NUL) at l.
pub open spec fn sh_add(b: Buf, s: Seq<char>) -> Buf {
    let need = b.content.len() as int + s.len() as int + 1;
    let g = sh_grow(b, need);
    Buf {
        content: b.content + s,
        cap: g.cap,
        alloc: g.alloc,
    }
}

pub proof fn add_content(b: Buf, s: Seq<char>)
    ensures
        sh_add(b, s).content == b.content + s,
{
    assert(sh_add(b, s).content == b.content + s);
}

pub proof fn add_wf(b: Buf, s: Seq<char>)
    requires
        wf(b),
        fits2(b.content.len() as int + s.len() as int + 1),
    ensures
        wf(sh_add(b, s)),
{
    let need = b.content.len() as int + s.len() as int + 1;
    let r = sh_add(b, s);
    assert((b.content + s).len() == b.content.len() + s.len());
    if need <= b.cap {
        assert(r.cap == b.cap);
        assert(r.alloc == b.alloc);
        assert(r.content.len() + 1 <= r.cap);
        assert(!r.alloc ==> r.content.len() == 0);
    } else {
        assert(r.cap == u64(2 * need));
        assert(u64(2 * need) == 2 * need);
        assert(2 * need >= need);
        assert(r.content.len() + 1 == need);
        assert(r.content.len() + 1 <= r.cap);
    }
}

// `_sh_addc`: grow to l+2, write c at l, NUL at l+1.
pub open spec fn sh_addc(b: Buf, c: char) -> Buf {
    let need = b.content.len() as int + 2;
    let g = sh_grow(b, need);
    Buf {
        content: b.content.push(c),
        cap: g.cap,
        alloc: g.alloc,
    }
}

pub proof fn addc_wf(b: Buf, c: char)
    requires
        wf(b),
        fits2(b.content.len() as int + 2),
    ensures
        wf(sh_addc(b, c)),
        sh_addc(b, c).content == b.content.push(c),
{
    let need = b.content.len() as int + 2;
    let r = sh_addc(b, c);
    assert((b.content.push(c)).len() == b.content.len() + 1);
    if need <= b.cap {
        assert(r.cap == b.cap && r.alloc == b.alloc);
        assert(r.content.len() + 1 <= r.cap);
        assert(!r.alloc ==> r.content.len() == 0);
    } else {
        assert(r.cap == u64(2 * need));
        assert(u64(2 * need) == 2 * need);
        assert(r.content.len() + 1 == need);
        assert(r.content.len() + 1 <= r.cap);
    }
}

// `_sh_fa` (the echo/colon fast path): grow to len+n+1, memcpy n at
// len, THEN write the NUL at len+n — byte-identical outcome to
// `_sh_add`, which is why one obligation family covers both
// emissions (and both mutants' boundary).
pub open spec fn sh_fa(b: Buf, s: Seq<char>) -> Buf {
    let need = b.content.len() as int + s.len() as int + 1;
    let g = sh_grow(b, need);
    Buf {
        content: b.content + s,
        cap: g.cap,
        alloc: g.alloc,
    }
}

pub proof fn fa_matches_add(b: Buf, s: Seq<char>)
    ensures
        sh_fa(b, s) == sh_add(b, s),
{
    assert(sh_fa(b, s) == sh_add(b, s));
}

// ── The doubling loop ────────────────────────────────────────────────

// `while (ncap < need) ncap *= 2;` — total by construction. The
// `2*ncap >= SZ` branch is where the SOURCE wraps: there the raw
// loop's product wraps to a smaller word and (pinned below in fuel
// form) eventually to 0, which is < need forever — the emitted loop
// never leaves. This model returns the stall point instead: a result
// `< need` IS the divergence witness, and every theorem below rules
// it out under its domain.
pub open spec fn dbl_until(ncap: int, need: int) -> int
    decreases SZ() - ncap,
{
    if ncap >= need {
        ncap
    } else if ncap <= 0 {
        ncap
    } else if 2 * ncap >= SZ() {
        ncap
    } else {
        let next = 2 * ncap;
        if next >= need {
            next
        } else {
            dbl_until(next, need)
        }
    }
}

// The raw loop for the wrap pin: `ncap *= 2` wrapping at 2^64,
// explicit fuel (the source is the fuel-less form; at the stall
// threshold it simply never returns).
pub open spec fn dbl_fuel(ncap: int, need: int, fuel: nat) -> int
    decreases fuel,
{
    if ncap >= need || fuel == 0 {
        ncap
    } else {
        dbl_fuel(u64(2 * ncap), need, (fuel - 1) as nat)
    }
}

// Doubling lands at >= need under the domain: base >= 1 (the
// source's `cap ? cap : 256`), need <= 2^63.
pub proof fn dbl_reaches(ncap: int, need: int)
    requires
        ncap >= 1,
        need >= 0,
        fits2(need),
    ensures
        dbl_until(ncap, need) >= need,
    decreases SZ() - ncap,
{
    if ncap >= need {
        assert(dbl_until(ncap, need) == ncap);
    } else if ncap <= 0 {
        assert(false);
    } else if 2 * ncap >= SZ() {
        // ncap < need <= 2^63 contradicts 2*ncap >= 2^64.
        assert(2 * ncap < SZ());
        assert(false);
    } else {
        let next = 2 * ncap;
        assert(next > ncap);
        assert(next < SZ());
        if next >= need {
            assert(dbl_until(ncap, need) == next);
            assert(next >= need);
        } else {
            assert(dbl_until(ncap, need) == dbl_until(next, need));
            dbl_reaches(next, need);
        }
    }
}

// Every value the loop computes is a representable size_t under the
// domain (no-wrap guarantee of the loop itself).
pub proof fn dbl_cap_ok(ncap: int, need: int)
    requires
        0 <= ncap < SZ(),
        need >= 0,
    ensures
        0 <= dbl_until(ncap, need) < SZ(),
    decreases SZ() - ncap,
{
    if ncap >= need {
        assert(dbl_until(ncap, need) == ncap);
    } else if ncap <= 0 {
        assert(dbl_until(ncap, need) == ncap);
    } else if 2 * ncap >= SZ() {
        assert(dbl_until(ncap, need) == ncap);
    } else {
        let next = 2 * ncap;
        assert(0 <= next < SZ());
        if next >= need {
            assert(dbl_until(ncap, need) == next);
        } else {
            assert(dbl_until(ncap, need) == dbl_until(next, need));
            dbl_cap_ok(next, need);
        }
    }
}

// Doubling never overshoots past 2×need (and keeps the base when the
// base already fits): the amortization claim behind "doubling".
pub proof fn dbl_bound(ncap: int, need: int)
    requires
        need >= 0,
    ensures
        dbl_until(ncap, need)
            <= (if ncap >= need {
                ncap
            } else {
                2 * need
            }),
    decreases SZ() - ncap,
{
    if ncap >= need {
        assert(dbl_until(ncap, need) == ncap);
    } else if ncap <= 0 {
        assert(dbl_until(ncap, need) == ncap);
        assert(ncap <= 0 && 0 <= 2 * need);
    } else if 2 * ncap >= SZ() {
        assert(dbl_until(ncap, need) == ncap);
        assert(ncap < need && need <= 2 * need);
    } else {
        let next = 2 * ncap;
        if next >= need {
            assert(dbl_until(ncap, need) == next);
            assert(2 * ncap <= 2 * need);
        } else {
            assert(dbl_until(ncap, need) == dbl_until(next, need));
            dbl_bound(next, need);
            assert(2 * ncap < 2 * need);
        }
    }
}

// Pinned thresholds — the load-bearing domain, exhibited, not
// assumed: past 2^63 the source's doubling makes no progress (this
// model stalls AT 2^63), and the RAW loop wraps all the way to 0 —
// 0 < need forever, i.e. the emitted `while` spins.
pub proof fn pin_vgrow_stall()
    ensures
        dbl_until(0x100int, 0x8000_0000_0000_0000int + 1) == 0x8000_0000_0000_0000int,
        dbl_until(0x100int, 0x8000_0000_0000_0000int + 1) < 0x8000_0000_0000_0000int + 1,
{
    assert(
        dbl_until(0x100int, 0x8000_0000_0000_0000int + 1) == 0x8000_0000_0000_0000int
    ) by (compute);
    assert(
        dbl_until(0x100int, 0x8000_0000_0000_0000int + 1) < 0x8000_0000_0000_0000int + 1
    );
}

pub proof fn pin_raw_wrap_to_zero()
    ensures
        dbl_fuel(0x100int, 0x8000_0000_0000_0000int + 1, 70) == 0int,
{
    assert(dbl_fuel(0x100int, 0x8000_0000_0000_0000int + 1, 70) == 0int) by (compute);
}

// ── B: `_sh_vgrow` ───────────────────────────────────────────────────

// Fast path `need <= cap && p`, else base = `cap ? cap : 256` and
// the doubling loop; realloc preserves the content.
pub open spec fn sh_vgrow(b: Buf, need: int) -> Buf {
    if need <= b.cap && b.alloc {
        b
    } else {
        let base = if b.cap > 0 {
            b.cap
        } else {
            256
        };
        let ncap = dbl_until(base, need);
        Buf {
            content: b.content,
            cap: ncap,
            alloc: true,
        }
    }
}

// THE deref-safety theorem every consumer stands on: after
// `_sh_vgrow` the buffer is allocated (p non-null) — whatever the
// input state, including the fresh {0,0} case the `&& b->p` half of
// the fast path exists for (need = 0 on a fresh buffer must NOT
// return NULL).
pub proof fn vgrow_nonnull(b: Buf, need: int)
    ensures
        sh_vgrow(b, need).alloc,
{
    if need <= b.cap && b.alloc {
        assert(sh_vgrow(b, need).alloc);
    } else {
        assert(sh_vgrow(b, need).alloc);
    }
}

pub proof fn vgrow_fastpath(b: Buf, need: int)
    ensures
        (need <= b.cap && b.alloc) ==> sh_vgrow(b, need) == b,
{
    if need <= b.cap && b.alloc {
        assert(sh_vgrow(b, need) == b);
    }
}

// Room + state preservation under the doubling domain.
pub proof fn vgrow_wf(b: Buf, need: int)
    requires
        wf(b),
        0 <= need,
        fits2(need),
    ensures
        wf(sh_vgrow(b, need)),
        sh_vgrow(b, need).cap >= need,
{
    if need <= b.cap && b.alloc {
        assert(sh_vgrow(b, need) == b);
    } else {
        let base = if b.cap > 0 {
            b.cap
        } else {
            256int
        };
        assert(base >= 1);
        dbl_reaches(base, need);
        dbl_cap_ok(base, need);
        let r = sh_vgrow(b, need);
        assert(r.cap >= need);
        if b.content.len() + 1 <= b.cap {
            // alloc'd: slow path means need > cap (the fast path's
            // first conjunct failed only if !alloc, or need > cap) —
            // either way content room carries into r.cap >= need.
            assert(r.content.len() + 1 <= b.cap || b.content.len() + 1 <= need);
        }
        assert(wf(r));
    }
}

// Amortization: a growth never allocates beyond max(base, 2·need)
// (or leaves the cap alone on the fast path).
pub proof fn vgrow_amortized(b: Buf, need: int)
    requires
        0 <= need,
    ensures
        sh_vgrow(b, need).cap
            <= (if need <= b.cap && b.alloc {
                b.cap
            } else {
                let base = if b.cap > 0 {
                    b.cap
                } else {
                    256int
                };
                if base > 2 * need {
                    base
                } else {
                    2 * need
                }
            }),
{
    if need <= b.cap && b.alloc {
        assert(sh_vgrow(b, need).cap == b.cap);
    } else {
        let base = if b.cap > 0 {
            b.cap
        } else {
            256int
        };
        dbl_bound(base, need);
        if base >= need {
            assert(dbl_until(base, need) <= base);
            assert(base <= (if base > 2 * need {
                base
            } else {
                2 * need
            }));
        } else {
            assert(dbl_until(base, need) <= 2 * need);
        }
    }
}

// ── The `snprintf_grow` loop ─────────────────────────────────────────

// What snprintf writes into a too-small buffer: at most cap-1 chars
// plus the NUL — total for every input (clamped, so the spec never
// indexes out of range).
pub open spec fn trunc_to(v: Seq<char>, n: int) -> Seq<char> {
    if n <= 0 {
        Seq::empty()
    } else if n >= v.len() {
        v
    } else {
        v.subrange(0, n)
    }
}

pub proof fn trunc_bound(v: Seq<char>, n: int)
    ensures
        trunc_to(v, n).len() <= (if n <= 0 {
            0int
        } else {
            n
        }),
{
    if n <= 0 {
        assert(trunc_to(v, n).len() == 0);
    } else if n >= v.len() {
        assert(trunc_to(v, n) == v);
        assert(trunc_to(v, n).len() == v.len());
        assert(v.len() <= n);
    } else {
        assert(trunc_to(v, n).len() == n);
    }
}

// Loop state: the buffer plus whether the loop has broken out.
pub struct SnprSt {
    pub b: Buf,
    pub broke: bool,
}

// One iteration over the formatter's next return `m` and the value
// `v` it formatted (the oracle contract `m == |v|` for m >= 0 lives
// in the theorems, as with every oracle here): `m < 0` encoding
// error → break, contents unspecified in C — modelled unchanged
// (see header); `m < cap` → break with the exact value written; else
// the too-small write (cap-1 chars) then grow to m+1 and continue.
pub open spec fn snpr_iter(s: SnprSt, m: int, v: Seq<char>) -> SnprSt {
    if m < 0 {
        // encoding error: the emitted `if (m < 0) break;` — break
        // with the contents C leaves unspecified, modelled unchanged
        // (see header).
        SnprSt {
            b: s.b,
            broke: true,
        }
    } else if m < s.b.cap {
        SnprSt {
            b: Buf {
                content: v,
                cap: s.b.cap,
                alloc: s.b.alloc,
            },
            broke: true,
        }
    } else {
        let g = sh_vgrow(s.b, m + 1);
        SnprSt {
            b: Buf {
                content: trunc_to(v, s.b.cap - 1),
                cap: g.cap,
                alloc: g.alloc,
            },
            broke: false,
        }
    }
}

// How many formatter calls the loop can survive before it MUST have
// broken: each non-break growth pushes cap past m >= cap, and int
// returns top out at imax — so cap > imax after at most
// `imax - cap + 2` growths, and the next return is < cap (break).
pub open spec fn need_steps(s: SnprSt) -> int {
    if s.broke {
        0
    } else if s.b.cap > imax() {
        1
    } else {
        imax() - s.b.cap + 2
    }
}

// The loop run over a finite sequence of formatter returns — ANY
// sequence: long enough means broken.
pub open spec fn snpr_run(s: SnprSt, ms: Seq<(int, Seq<char>)>) -> SnprSt
    decreases ms.len(),
{
    if s.broke || ms.len() == 0 {
        s
    } else {
        let r = snpr_iter(s, ms[0].0, ms[0].1);
        if r.broke {
            r
        } else {
            snpr_run(r, ms.drop_first())
        }
    }
}

// Termination of the emitted `for (;;)` over ANY int-typed return
// sequence, plus wf preservation throughout: within need_steps calls
// the loop breaks.
pub proof fn snpr_run_props(s: SnprSt, ms: Seq<(int, Seq<char>)>)
    requires
        wf(s.b),
        (forall|i: int| 0 <= i < ms.len() ==> ms[i].0 <= imax()),
        (forall|i: int| 0 <= i < ms.len() ==> ms[i].0 < 0 || ms[i].0 == ms[i].1.len()),
        ms.len() as int >= need_steps(s),
    ensures
        snpr_run(s, ms).broke,
        wf(snpr_run(s, ms).b),
    decreases ms.len(),
{
    if s.broke {
        assert(snpr_run(s, ms).broke);
        assert(wf(snpr_run(s, ms).b));
    } else if ms.len() == 0 {
        assert(need_steps(s) >= 1);
        assert(false);
    } else {
        let m = ms[0].0;
        let v = ms[0].1;
        assert(0 <= 0 < ms.len());
        assert(m <= imax());
        if m < 0 {
            assert(!s.broke && ms.len() != 0);
            assert(snpr_run(s, ms) == snpr_iter(s, m, v));
            assert(snpr_run(s, ms).broke);
            assert(wf(snpr_run(s, ms).b));
        } else if m < s.b.cap {
            assert(m == v.len());
            assert(v.len() + 1 <= s.b.cap);
            assert(snpr_run(s, ms) == snpr_iter(s, m, v));
            assert(snpr_run(s, ms).broke);
            assert(wf(snpr_run(s, ms).b));
        } else {
            // grow step: cap strictly increases, wf holds, and the
            // remaining budget shrinks by at least one call.
            let r = snpr_progress(s, m, v);
            assert(!r.broke);
            assert(r.b.cap > s.b.cap);
            assert(wf(r.b));
            assert(need_steps(r) <= need_steps(s) - 1);
            assert(snpr_run(s, ms) == snpr_run(r, ms.drop_first()));
            snpr_run_props(r, ms.drop_first());
        }
    }
}

// Progress: a non-break iteration strictly increases cap (need =
// m+1 > cap) and keeps the state well formed — the monotone-cap
// property the termination argument eats.
pub proof fn snpr_progress(s: SnprSt, m: int, v: Seq<char>) -> (r: SnprSt)
    requires
        wf(s.b),
        m >= s.b.cap,
        m >= 0,
        m <= imax(),
    ensures
        r == snpr_iter(s, m, v),
        !r.broke,
        r.b.cap > s.b.cap,
        wf(r.b),
{
    let need = m + 1;
    assert(need > s.b.cap);
    assert(0 <= need);
    assert(fits2(need));
    vgrow_wf(s.b, need);
    let g = sh_vgrow(s.b, need);
    assert(g.cap >= need);
    assert(g.cap > s.b.cap);
    assert(wf(g));
    let r = snpr_iter(s, m, v);
    assert(!r.broke);
    assert(r.b.cap == g.cap && r.b.alloc == g.alloc);
    // content = trunc(v, cap-1): len <= cap-1 when cap >= 1, empty
    // when cap == 0 — either way r's room and pointer laws hold.
    trunc_bound(v, s.b.cap - 1);
    if s.b.cap >= 1 {
        assert(trunc_to(v, s.b.cap - 1).len() <= s.b.cap - 1);
        assert(r.b.content.len() + 1 <= s.b.cap);
    } else {
        assert(r.b.content.len() == 0);
    }
    assert(r.b.content.len() + 1 <= g.cap || r.b.content.len() == 0);
    assert(wf(r.b));
    r
}

// The break condition yields the EXACT formatted value, in bounds
// (m < cap means snprintf wrote all m chars + the NUL inside the
// cap) — what makes `.p` the right answer rather than a truncated
// one.
pub proof fn snpr_break_exact(s: SnprSt, m: int, v: Seq<char>)
    requires
        m >= 0,
        m == v.len() as int,
        m < s.b.cap,
    ensures
        snpr_iter(s, m, v).broke,
        snpr_iter(s, m, v).b.content == v,
        v.len() as int + 1 <= s.b.cap,
{
    assert(v.len() + 1 <= s.b.cap);
    assert(snpr_iter(s, m, v).broke);
    assert(snpr_iter(s, m, v).b.content == v);
}

// The initial `_sh_vgrow(&{t}, 256)` on a fresh temp: p non-null
// with size >= 256 — the state the first snprintf writes into.
pub proof fn snpr_init_room(b: Buf, need: int)
    requires
        wf(b),
        0 <= need,
        fits2(need),
    ensures
        sh_vgrow(b, need).alloc,
        sh_vgrow(b, need).cap >= need,
{
    vgrow_wf(b, need);
}

// ── C: `_sh_grow_str` / typed grows ──────────────────────────────────

// A string vec: `slots` mirrors the allocated pointer region
// (slots.len() == cap); `None` is a NULL slot.
pub struct SVec {
    pub slots: Seq<Option<Seq<char>>>,
    pub cap: int,
}

// `ncap = cap ? cap * 2 : 16`, then the str variant ZERO-FILLS the
// fresh region [ocap, ncap). Old slots survive realloc (allocator
// premise). The gap clamp keeps the spec total off-domain (a
// wrapped cap would ask for a negative-length fresh region — exactly
// the stall class the pins exhibit; callers hold `fits2`).
// Fresh-region gap of one growth: ncap - cap, clamped total
// off-domain (a wrapped cap would ask for a negative-length fresh
// region — exactly the stall class the pins exhibit; callers hold
// `fits2`).
pub open spec fn gap_of(sv: SVec) -> nat {
    let ncap = if sv.cap > 0 {
        u64(2 * sv.cap)
    } else {
        16
    };
    if ncap > sv.cap {
        (ncap - sv.cap) as nat
    } else {
        0nat
    }
}

// `ncap = cap ? cap * 2 : 16`, then the str variant ZERO-FILLS the
// fresh region [ocap, ncap). Old slots survive realloc (allocator
// premise).
pub open spec fn grow_str(sv: SVec) -> SVec {
    let ocap = sv.cap;
    let ncap = if sv.cap > 0 {
        u64(2 * sv.cap)
    } else {
        16
    };
    SVec {
        slots: sv.slots + Seq::new(gap_of(sv), |_i: int| Option::<Seq<char>>::None),
        cap: ncap,
    }
}

// Exact doubling cap + byte-size fit (the domain: the source does
// NOT guard `ncap * sizeof *np` — honest proviso, and mutant M3's
// contrast is D, which does guard it).
pub proof fn grow_str_cap(sv: SVec)
    requires
        sv.cap >= 0,
        sv.cap == sv.slots.len(),
        fits2(sv.cap),
        2 * sv.cap * 8 < SZ(),
    ensures
        grow_str(sv).cap == (if sv.cap > 0 {
            2 * sv.cap
        } else {
            16
        }),
        0 <= grow_str(sv).cap,
        grow_str(sv).cap * 8 < SZ(),
{
    if sv.cap > 0 {
        assert(u64(2 * sv.cap) == 2 * sv.cap);
    }
}

// The safety property the zero-fill exists for: every slot of the
// FRESH region is NULL — so a later `free` of a sparse slot, or a
// len-guarded read, never touches garbage — and every OLD slot is
// preserved.
pub proof fn grow_str_zeroed(sv: SVec)
    requires
        sv.cap >= 0,
        sv.cap == sv.slots.len(),
        fits2(sv.cap),
    ensures
        (forall|i: int| sv.cap <= i < grow_str(sv).cap ==> grow_str(sv).slots[i].is_None()),
        (forall|i: int| 0 <= i < sv.cap ==> grow_str(sv).slots[i] == sv.slots[i]),
{
    let ocap = sv.cap;
    let ncap = if sv.cap > 0 {
        u64(2 * sv.cap)
    } else {
        16
    };
    if sv.cap > 0 {
        assert(u64(2 * sv.cap) == 2 * sv.cap);
        assert(ncap > ocap);
    } else {
        assert(ncap == 16 && ocap == 0);
    }
    let r = grow_str(sv);
    assert(r.slots.len() == ncap);
    assert((sv.slots + Seq::new(gap_of(sv), |_i: int| Option::<Seq<char>>::None)).len()
        == sv.slots.len() + gap_of(sv));
}

// The typed grows share C's arithmetic minus the zero-fill (reads
// are len-guarded by the render layer — honest, not modelled).
pub proof fn grow_t_cap(cap: int)
    requires
        cap >= 0,
        fits2(cap),
        2 * cap * 8 < SZ(),
    ensures
        (if cap > 0 {
            u64(2 * cap)
        } else {
            16
        }) * 8 < SZ(),
{
    if cap > 0 {
        assert(u64(2 * cap) == 2 * cap);
    }
}

// The vec-slot guard: `while (len + 1 > cap) _sh_grow_str(&id,…)`,
// doubling to termination under `len + 1 <= 2^63`; the result has
// room for the `id[id_len++] = …` store that follows it.
pub open spec fn vecgrow(cap: int, len: int) -> int
    decreases SZ() - cap,
{
    if len + 1 <= cap {
        cap
    } else if cap <= 0 {
        // first growth from {0}: ncap = 16, then the emitted `while`
        // re-checks the condition before growing again.
        if 16 > len + 1 {
            16
        } else {
            vecgrow(16, len)
        }
    } else if 2 * cap >= SZ() {
        cap
    } else {
        let next = 2 * cap;
        if next > len + 1 {
            next
        } else {
            vecgrow(next, len)
        }
    }
}

pub proof fn vecgrow_post(cap: int, len: int)
    requires
        0 <= cap < SZ(),
        len >= 0,
        len + 1 <= SZ() / 2,
    ensures
        vecgrow(cap, len) >= len + 1,
        vecgrow(cap, len) < SZ(),
    decreases SZ() - cap,
{
    if len + 1 <= cap {
        assert(vecgrow(cap, len) == cap);
    } else if cap <= 0 {
        // first growth from {0}: ncap = 16, then the emitted `while`
        // RE-CHECKS — recurse, don't assume 16 suffices.
        assert(vecgrow(cap, len) == (if 16 > len + 1 {
            16int
        } else {
            vecgrow(16int, len)
        }));
        if 16 > len + 1 {
            assert(vecgrow(cap, len) == 16);
            assert(16 >= len + 1);
        } else {
            vecgrow_post(16int, len);
        }
    } else if 2 * cap >= SZ() {
        assert(2 * cap < SZ());
        assert(false);
    } else {
        let next = 2 * cap;
        assert(0 < next < SZ());
        if next > len + 1 {
            assert(vecgrow(cap, len) == next);
            assert(next >= len + 1);
        } else {
            assert(vecgrow(cap, len) == vecgrow(next, len));
            vecgrow_post(next, len);
        }
    }
}

// ── D: `_sh_reserve` ─────────────────────────────────────────────────

// Exact reserve. None = the GUARD's exit(127) (abort) — the guard
// fires BEFORE any multiply, which is the whole point of D. (elem is
// 1/2/4/8 in every emission; the divisor is never 0.)
pub open spec fn reserve(cap: int, need: int, elem: int) -> Option<int> {
    if need <= cap {
        Option::Some(cap)
    } else if need > (SZ() - 1) / elem {
        Option::None
    } else {
        Option::Some(need)
    }
}

// The guard fires exactly on oversized requests.
pub proof fn reserve_guard(cap: int, need: int, elem: int)
    requires
        elem == 1 || elem == 2 || elem == 4 || elem == 8,
        need > (SZ() - 1) / elem,
        need > cap,
    ensures
        reserve(cap, need, elem).is_None(),
{
    assert(reserve(cap, need, elem).is_None());
}

// A request that PASSES the guard multiplies EXACTLY — no wrap and
// no domain proviso of its own: `need <= (2^64-1)/elem ==> need*elem
// <= 2^64-1`. This is the theorem A/B/C cannot state unconditionally
// — their asymmetry.
pub proof fn reserve_no_wrap(cap: int, need: int, elem: int)
    requires
        elem == 1 || elem == 2 || elem == 4 || elem == 8,
        need > cap,
        0 <= need,
        need <= (SZ() - 1) / elem,
    ensures
        reserve(cap, need, elem) == Option::Some(need),
        need * elem < SZ(),
{
    if elem == 8 {
        assert(8 * ((SZ() - 1) / 8) <= SZ() - 1);
    } else if elem == 4 {
        assert(4 * ((SZ() - 1) / 4) <= SZ() - 1);
    } else if elem == 2 {
        assert(2 * ((SZ() - 1) / 2) <= SZ() - 1);
    } else {
        assert(((SZ() - 1) / 1) <= SZ() - 1);
    }
    assert(need * elem <= SZ() - 1);
    assert(reserve(cap, need, elem) == Option::Some(need));
}

// ── `_sh_fgets_line`: capacity + terminator discipline ───────────────

// After `_sh_vgrow(vb, n + 512)`: cap >= n+512 and p non-null, so
// the fgets size (int)(cap - n) is >= 512 and the NUL fgets writes
// lands at n + (cap-n) - 1 = cap - 1 — inside the allocation. (The
// (int) cast above INT_MAX is implementation-defined — honest domain
// `cap - n <= imax()`: a >2GiB line buffer, not a corpus shape.)
pub proof fn fgl_capacity(n: int, b: Buf)
    requires
        wf(b),
        n >= 0,
        n + 512 < SZ() / 2,
        sh_vgrow(b, n + 512).cap - n <= imax(),
    ensures
        sh_vgrow(b, n + 512).alloc,
        sh_vgrow(b, n + 512).cap >= n + 512,
        sh_vgrow(b, n + 512).cap - n >= 512,
        n + (sh_vgrow(b, n + 512).cap - n) - 1 == sh_vgrow(b, n + 512).cap - 1,
        sh_vgrow(b, n + 512).cap - 1 < sh_vgrow(b, n + 512).cap,
{
    let r = sh_vgrow(b, n + 512);
    vgrow_wf(b, n + 512);
    assert(r.cap >= n + 512);
    assert(r.alloc);
    assert(r.cap - n >= 512);
}

// The line's final ` _sh_vgrow(vb, n + 1); vb->p[n] = 0;`: room for
// the terminator at index n, in bounds, p non-null.
pub proof fn fgl_final_nul(n: int, b: Buf)
    requires
        wf(b),
        n >= 0,
        n + 1 < SZ() / 2,
    ensures
        sh_vgrow(b, n + 1).cap >= n + 1,
        n < sh_vgrow(b, n + 1).cap,
        sh_vgrow(b, n + 1).alloc,
{
    let r = sh_vgrow(b, n + 1);
    vgrow_wf(b, n + 1);
    assert(r.cap >= n + 1);
    assert(n < r.cap);
    assert(r.alloc);
}

// ── Negative controls: the three mutants ─────────────────────────────

// M1 — `_sh_add` with the `+1` dropped from the need. At the
// boundary cap == l+n the fast path returns without growing, and
// the memcpy's NUL lands one byte past the allocation.
pub open spec fn sh_add_mut(b: Buf, s: Seq<char>) -> Buf {
    let need = b.content.len() as int + s.len() as int;
    let g = sh_grow(b, need);
    Buf {
        content: b.content + s,
        cap: g.cap,
        alloc: g.alloc,
    }
}

pub proof fn mutant_add_no_plus1()
{
    let b = Buf {
        content: seq!['a'],
        cap: 2,
        alloc: true,
    };
    assert(wf(b));
    let s = seq!['x'];
    let r = sh_add_mut(b, s);
    // need == 2 <= cap: the fast path does NOT grow …
    assert(r.cap == 2);
    // … so the NUL at content.len() == 2 is OUT of bounds: the
    // bound the real `_sh_add` proves (`len + 1 <= cap`) fails.
    assert(r.content.len() as int + 1 == 3);
    assert(!(r.content.len() as int + 1 <= r.cap));
    // and the real add on the same state grows and fits:
    let g = sh_add(b, s);
    assert(g.cap == 6);
    assert(g.content.len() as int + 1 <= g.cap);
}

// M2 — the zero-fill deleted: the fresh region keeps garbage (a
// dangling non-NULL marker), i.e. the `grow_str_zeroed`
// postcondition fails at the first fresh slot.
pub open spec fn grow_str_mut(sv: SVec) -> SVec {
    let ocap = sv.cap;
    let ncap = if sv.cap > 0 {
        u64(2 * sv.cap)
    } else {
        16
    };
    SVec {
        slots: sv.slots
            + Seq::new(gap_of(sv), |_i: int| Option::Some(Seq::empty())),
        cap: ncap,
    }
}

pub proof fn mutant_grow_str_no_zero(sv: SVec)
    requires
        sv.cap >= 1,
        sv.cap == sv.slots.len(),
        fits2(sv.cap),
    ensures
        !(forall|i: int| sv.cap <= i < grow_str_mut(sv).cap
                ==> grow_str_mut(sv).slots[i].is_None()),
{
    assert(u64(2 * sv.cap) == 2 * sv.cap);
    let r = grow_str_mut(sv);
    assert(r.cap == 2 * sv.cap);
    assert(r.slots.len() == r.cap);
    assert(r.slots[sv.cap].is_Some());
}

// M3 — the reserve guard deleted: 2^61 × 8 = 2^64 wraps to 0, so
// the realloc would be sized 0 bytes for a 2^61-element request.
pub proof fn mutant_reserve_unguarded()
    ensures
        !(0x8000_0000_0000_0000int * 8 < SZ()),
        u64(0x8000_0000_0000_0000int * 8) == 0int,
{
    assert(0x8000_0000_0000_0000int * 8 == 0x4_0000_0000_0000_0000int);
    assert(u64(0x8000_0000_0000_0000int * 8) == 0int);
}

// ── Fidelity pins (measured shapes) ──────────────────────────────────

// The probe shape: `_sh_add` on a fresh `_sh_cmd` grows to exactly
// 2*(l+n+1) and holds the appended bytes — the emitted protocol
// exercised end to end on the smallest real state.
pub proof fn pin_add_fresh()
    ensures
        sh_add(Buf { content: Seq::empty(), cap: 0, alloc: false }, seq!['h', 'i']).content
            == seq!['h', 'i'],
        sh_add(Buf { content: Seq::empty(), cap: 0, alloc: false }, seq!['h', 'i']).cap == 6,
        sh_add(Buf { content: Seq::empty(), cap: 0, alloc: false }, seq!['h', 'i']).alloc,
{
    let r = sh_add(Buf { content: Seq::empty(), cap: 0, alloc: false }, seq!['h', 'i']);
    assert(r.content == seq!['h', 'i']);
    assert(r.cap == u64(6int));
    assert(r.cap == 6);
    assert(r.alloc);
}

// The `snprintf_grow` initial `_sh_vgrow(&t, 256)` on a fresh temp:
// p non-null with size exactly 256 — the state the first snprintf
// writes into.
pub proof fn pin_snpr_init()
    ensures
        sh_vgrow(Buf { content: Seq::empty(), cap: 0, alloc: false }, 256).alloc,
        sh_vgrow(Buf { content: Seq::empty(), cap: 0, alloc: false }, 256).cap == 256,
{
    let r = sh_vgrow(Buf { content: Seq::empty(), cap: 0, alloc: false }, 256);
    assert(r.alloc);
    assert(r.cap == dbl_until(256, 256));
    assert(dbl_until(256, 256) == 256);
}

fn main() {}

} // verus!
