//! Phase-0 profiling (`SH2_PROFILE=1`): loop trips, branch-arm entries,
//! value-magnitude buckets, array lengths — dumped as JSON at exit.
//!
//! Vehicle: hand-built JSON IR (frontend A1 shape, mirrored from a
//! verified render) → ingress → render → gcc+run → exact JSON asserts.
//! This binary is its own process, so `SH2_PROFILE` may be set freely —
//! but tests within it share env: a file-local lock serializes them.

use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;

static TEST_LOCK: Mutex<()> = Mutex::new(());
static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(9000);

fn prog_json() -> String {
    // i=0; xs=[]; while i<5 { if i%2==0 {xs.append(i)} else {xs.append(i*1000)}; i++ }; echo done
    // (shIR lifted verbatim from a verified render — hand-written shapes kept drifting.)
    r##"{
 "type": "Program",
 "contract_version": 1,
 "imports": [],
 "requires": [],
 "subs": [],
 "var_types": [
  {
   "name": "i",
   "type": "Int"
  }
 ],
 "stmts": [
  {
   "expr": {
    "args": [
     {
      "style": "DoubleQuoted",
      "type": "Str",
      "value": "xs"
     },
     {
      "elements": [],
      "type": "Array"
     }
    ],
    "func": "setArray",
    "purity": "Emulable",
    "type": "Call"
   },
   "targets": [
    {
     "indices": [],
     "sigil": null,
     "var": "xs"
    }
   ],
   "type": "Assign"
  },
  {
   "expr": {
    "style": "DoubleQuoted",
    "type": "Str",
    "value": "0"
   },
   "targets": [
    {
     "indices": [],
     "sigil": null,
     "var": "i"
    }
   ],
   "type": "Assign"
  },
  {
   "body": [
    {
     "cond": {
      "args": [
       {
        "style": "DoubleQuoted",
        "type": "Str",
        "value": "$((i % 2)) -eq 0"
       }
      ],
      "func": "test",
      "purity": "Emulable",
      "type": "Call"
     },
     "else": [
      {
       "expr": {
        "args": [
         {
          "style": "DoubleQuoted",
          "type": "Str",
          "value": "xs"
         },
         {
          "elements": [
           {
            "ast": {
             "lhs": {
              "name": "i",
              "type": "Var"
             },
             "op": "*",
             "rhs": {
              "type": "Num",
              "value": 1000
             },
             "type": "Bin"
            },
            "type": "Arith"
           }
          ],
          "type": "Array"
         }
        ],
        "func": "setArrayAppend",
        "purity": "Emulable",
        "type": "Call"
       },
       "targets": [
        {
         "indices": [],
         "sigil": null,
         "var": "xs"
        }
       ],
       "type": "Assign"
      },
      {
       "expr": {
        "args": [
         {
          "style": "DoubleQuoted",
          "type": "Str",
          "value": ")"
         },
         {
          "elements": [],
          "type": "Array"
         }
        ],
        "func": "exec",
        "purity": "Spawn",
        "type": "Call"
       },
       "type": "Expr"
      }
     ],
     "elsifs": [],
     "then": [
      {
       "expr": {
        "args": [
         {
          "style": "DoubleQuoted",
          "type": "Str",
          "value": "xs"
         },
         {
          "elements": [
           {
            "args": [
             {
              "args": [
               {
                "style": "DoubleQuoted",
                "type": "Str",
                "value": "i"
               }
              ],
              "func": "getVar",
              "purity": "Emulable",
              "type": "Call"
             }
            ],
            "func": "split",
            "purity": "PureCpu",
            "type": "Call"
           }
          ],
          "type": "Array"
         }
        ],
        "func": "setArrayAppend",
        "purity": "Emulable",
        "type": "Call"
       },
       "targets": [
        {
         "indices": [],
         "sigil": null,
         "var": "xs"
        }
       ],
       "type": "Assign"
      }
     ],
     "type": "If"
    },
    {
     "expr": {
      "ast": {
       "lhs": {
        "name": "i",
        "type": "Var"
       },
       "op": "+",
       "rhs": {
        "type": "Num",
        "value": 1
       },
       "type": "Bin"
      },
      "type": "Arith"
     },
     "targets": [
      {
       "indices": [],
       "sigil": null,
       "var": "i"
      }
     ],
     "type": "Assign"
    }
   ],
   "cond": {
    "args": [
     {
      "style": "DoubleQuoted",
      "type": "Str",
      "value": "$i -lt 5"
     }
    ],
    "func": "test",
    "purity": "Emulable",
    "type": "Call"
   },
   "runs": true,
   "type": "While"
  },
  {
   "expr": {
    "args": [
     {
      "style": "DoubleQuoted",
      "type": "Str",
      "value": "echo"
     },
     {
      "elements": [
       {
        "parts": [
         {
          "kind": "lit",
          "text": "len="
         },
         {
          "expr": {
           "args": [
            {
             "args": [
              {
               "style": "DoubleQuoted",
               "type": "Str",
               "value": "slice"
              },
              {
               "style": "DoubleQuoted",
               "type": "Str",
               "value": "#xs"
              },
              {
               "style": "DoubleQuoted",
               "type": "Str",
               "value": "@"
              },
              {
               "style": "DoubleQuoted",
               "type": "Str",
               "value": ""
              }
             ],
             "func": "param",
             "purity": "PureCpu",
             "type": "Call"
            }
           ],
           "func": "join",
           "purity": "PureCpu",
           "type": "Call"
          },
          "kind": "expr"
         },
         {
          "kind": "lit",
          "text": " i="
         },
         {
          "expr": {
           "args": [
            {
             "style": "DoubleQuoted",
             "type": "Str",
             "value": "i"
            }
           ],
           "func": "getVar",
           "purity": "Emulable",
           "type": "Call"
          },
          "kind": "expr"
         }
        ],
        "type": "Interpolate"
       }
      ],
      "type": "Array"
     }
    ],
    "func": "builtin",
    "purity": "Emulable",
    "type": "Call"
   },
   "type": "Expr"
  }
 ]
}"##.to_string()
}

/// Render with the profile env set (or cleared when `enabled` is false).
/// Returns the C source. Holds the file-local lock via the caller.
fn render_c(json: &str, enabled: bool, out: &std::path::Path) -> String {
    if enabled {
        std::env::set_var("SH2_PROFILE", "1");
        std::env::set_var("SH2_PROFILE_OUT", out);
    } else {
        std::env::remove_var("SH2_PROFILE");
        std::env::remove_var("SH2_PROFILE_OUT");
    }
    let prog = debashl::shir_json_in::shir_json_to_ir(json).expect("test IR must ingress");
    debashl::c_backend::shir_to_c(&prog)
}

fn tmp(name: &str) -> std::path::PathBuf {
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    std::env::temp_dir().join(format!("sh2prof_{name}_{n}_{}", std::process::id()))
}

fn parse_report(path: &std::path::Path) -> J {
    let text = std::fs::read_to_string(path).expect("profile JSON must exist");
    serde_json_like::parse(&text)
}

// Minimal JSON value model (no serde dependency in this crate's dev-deps).
#[derive(Debug, Clone)]
enum J {
    Null,
    Bool(bool),
    Num(i128),
    Str(String),
    Arr(Vec<J>),
    Obj(HashMap<String, J>),
}

mod serde_json_like {
    use super::J;
    use std::collections::HashMap;
    pub fn parse(text: &str) -> J {
        let mut p = Parser { b: text.as_bytes(), i: 0 };
        p.ws();
        let v = p.value();
        p.ws();
        assert!(p.i == p.b.len(), "trailing JSON at {}", p.i);
        v
    }
    struct Parser<'a> {
        b: &'a [u8],
        i: usize,
    }
    impl<'a> Parser<'a> {
        fn ws(&mut self) {
            while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
                self.i += 1;
            }
        }
        fn lit(&mut self, s: &str) {
            assert!(self.b[self.i..].starts_with(s.as_bytes()), "want {s} at {}", self.i);
            self.i += s.len();
        }
        fn value(&mut self) -> J {
            match self.b[self.i] {
                b'{' => {
                    self.i += 1;
                    let mut m = HashMap::new();
                    self.ws();
                    if self.b[self.i] == b'}' {
                        self.i += 1;
                        return J::Obj(m);
                    }
                    loop {
                        self.ws();
                        let k = self.string();
                        self.ws();
                        self.lit(":");
                        self.ws();
                        m.insert(k, self.value());
                        self.ws();
                        match self.b[self.i] {
                            b',' => {
                                self.i += 1;
                            }
                            b'}' => {
                                self.i += 1;
                                break;
                            }
                            c => panic!("bad obj char {c} at {}", self.i),
                        }
                    }
                    J::Obj(m)
                }
                b'[' => {
                    self.i += 1;
                    let mut v = Vec::new();
                    self.ws();
                    if self.b[self.i] == b']' {
                        self.i += 1;
                        return J::Arr(v);
                    }
                    loop {
                        self.ws();
                        v.push(self.value());
                        self.ws();
                        match self.b[self.i] {
                            b',' => {
                                self.i += 1;
                            }
                            b']' => {
                                self.i += 1;
                                break;
                            }
                            c => panic!("bad arr char {c} at {}", self.i),
                        }
                    }
                    J::Arr(v)
                }
                b'"' => J::Str(self.string()),
                b't' => {
                    self.lit("true");
                    J::Bool(true)
                }
                b'f' => {
                    self.lit("false");
                    J::Bool(false)
                }
                b'n' => {
                    self.lit("null");
                    J::Null
                }
                _ => {
                    let s = self.i;
                    while self.i < self.b.len() && !matches!(self.b[self.i], b',' | b']' | b'}' | b' ' | b'\t' | b'\n' | b'\r') {
                        self.i += 1;
                    }
                    let n: i128 = std::str::from_utf8(&self.b[s..self.i]).unwrap().parse().unwrap();
                    J::Num(n)
                }
            }
        }
        fn string(&mut self) -> String {
            assert_eq!(self.b[self.i], b'"');
            self.i += 1;
            let mut o = String::new();
            loop {
                match self.b[self.i] {
                    b'"' => {
                        self.i += 1;
                        break;
                    }
                    b'\\' => {
                        self.i += 1;
                        match self.b[self.i] {
                            b'"' => o.push('"'),
                            b'\\' => o.push('\\'),
                            b'n' => o.push('\n'),
                            b't' => o.push('\t'),
                            b'r' => o.push('\r'),
                            b'u' => {
                                let h = std::str::from_utf8(&self.b[self.i + 1..self.i + 5]).unwrap();
                                let c = u32::from_str_radix(h, 16).unwrap();
                                o.push(char::from_u32(c).unwrap());
                                self.i += 4;
                            }
                            c => panic!("bad escape {c}"),
                        }
                        self.i += 1;
                    }
                    c => {
                        o.push(c as char);
                        self.i += 1;
                    }
                }
            }
            o
        }
    }
}

impl J {
    fn get(&self, k: &str) -> &J {
        match self {
            J::Obj(m) => m.get(k).unwrap_or_else(|| panic!("missing key {k} in {self:?}")),
            _ => panic!("not an object: {self:?}"),
        }
    }
    fn num(&self) -> i128 {
        match self {
            J::Num(n) => *n,
            _ => panic!("not a number: {self:?}"),
        }
    }
    fn arr(&self) -> &Vec<J> {
        match self {
            J::Arr(v) => v,
            _ => panic!("not an array: {self:?}"),
        }
    }
}

/// Default-off render: no `_prof_` symbols at all.
#[test]
fn profile_off_is_clean() {
    let _g = TEST_LOCK.lock().unwrap();
    let c = render_c(&prog_json(), false, &tmp("off"));
    assert!(
        !c.contains("_prof_"),
        "profile symbols leaked into default render"
    );
    assert!(!c.contains("_sh_prof_dump"), "dump fn leaked into default render");
}

/// Profile render is deterministic: same input → byte-identical C
/// (site ids are positional in render order).
#[test]
fn profile_render_deterministic() {
    let _g = TEST_LOCK.lock().unwrap();
    let o1 = tmp("det1.json");
    let o2 = tmp("det2.json");
    let a = render_c(&prog_json(), true, &o1);
    let b = render_c(&prog_json(), true, &o2);
    // (out paths differ → the baked _prof_out line differs; the rest must not)
    let norm = |s: &str| {
        s.lines()
            .filter(|l| !l.contains("_prof_out ="))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(norm(&a), norm(&b), "profile render not deterministic");
}

/// Golden run: compile + execute + assert every reported count.
#[test]
fn profile_golden_counts() {
    let _g = TEST_LOCK.lock().unwrap();
    let dir = tmp("gold");
    std::fs::create_dir_all(&dir).unwrap();
    let json_path = dir.join("p.json");
    let c = render_c(&prog_json(), true, &json_path);
    let c_path = dir.join("p.c");
    std::fs::write(&c_path, &c).unwrap();
    let bin = dir.join("p");
    let cc = Command::new("cc")
        .args(["-O1", "-o"])
        .arg(&bin)
        .arg(&c_path)
        .output()
        .expect("cc must run");
    assert!(cc.status.success(), "cc failed: {}", String::from_utf8_lossy(&cc.stderr));
    let run = Command::new(&bin).output().expect("binary must run");
    assert!(run.status.success(), "run failed: {}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "len=5 i=5");

    let r = parse_report(&json_path);
    assert_eq!(r.get("schema").num(), 1);
    match r.get("source_hash") {
        J::Str(h) => assert!(
            h.len() == 16 && h.chars().all(|c| c.is_ascii_hexdigit()),
            "source_hash must be 16 hex chars: {h}"
        ),
        other => panic!("source_hash not a string: {other:?}"),
    }
    let loops = r.get("loops").arr();
    assert_eq!(loops.len(), 1, "one while loop: {loops:?}");
    assert_eq!(loops[0].get("execs").num(), 1);
    assert_eq!(loops[0].get("trip_sum").num(), 5);
    assert_eq!(loops[0].get("trip_max").num(), 5);
    let trips: Vec<i128> = loops[0].get("trips").arr().iter().map(|j| j.num()).collect();
    assert_eq!(trips, vec![5]);
    let arms = r.get("branches").arr();
    assert_eq!(arms.len(), 2, "then+else: {arms:?}");
    let mut execs: Vec<i128> = arms.iter().map(|j| j.get("execs").num()).collect();
    execs.sort();
    assert_eq!(execs, vec![2, 3], "then=3 (i=0,2,4), else=2 (i=1,3)");
    // xs is a string array (split() appends) → length sites only, no mags
    let lens = r.get("lens").arr();
    assert_eq!(lens.len(), 2, "one site per append statement: {lens:?}");
    let mut maxes: Vec<i128> = lens.iter().map(|j| j.get("len_max").num()).collect();
    maxes.sort();
    assert_eq!(maxes, vec![4, 5]);
    let mut apps: Vec<i128> = lens.iter().map(|j| j.get("appends").num()).collect();
    apps.sort();
    assert_eq!(apps, vec![2, 3]);
    // i assignments: init bucket 0 (value 0), incr bucket 1 (1..5)
    let mags = r.get("mags").arr();
    assert_eq!(mags.len(), 2, "two int assigns: {mags:?}");
    let mut buckets: Vec<i128> = mags.iter().map(|j| j.get("max_bucket").num()).collect();
    buckets.sort();
    assert_eq!(buckets, vec![0, 1]);
}

/// The staleness key moves with the source: different programs hash
/// differently (the merge tool refuses cross-hash merges).
#[test]
fn profile_hash_tracks_source() {
    let _g = TEST_LOCK.lock().unwrap();
    let h = |json: &str| {
        let c = render_c(json, true, &tmp("h.json"));
        c.lines()
            .find(|l| l.contains("_prof_hash ="))
            .expect("hash line")
            .to_string()
    };
    let h1 = h(&prog_json());
    let other = prog_json().replace("$i -lt 5", "$i -lt 6");
    let h2 = h(&other);
    assert_ne!(h1, h2, "hash must track the source");
    let h1b = h(&prog_json());
    assert_eq!(h1, h1b, "same source must hash stably");
}

/// SH2_ASSUME_OBSERVED_WIDTHS (docs/DUAL_LOOPS.MD §4): a observed
/// magnitude bucket seeds the range, and the narrowed store carries a
/// runtime guard. The profile's hash must match or the render refuses.
#[test]
fn assume_observed_width_guard() {
    let _g = TEST_LOCK.lock().unwrap();
    // Take a profiling render to read the program's own staleness key.
    let pc = render_c(&prog_json(), true, &tmp("aw_prof.json"));
    let hash = pc
        .lines()
        .find(|l| l.contains("_prof_hash ="))
        .expect("hash line")
        .split("0x")
        .nth(1)
        .expect("hex")
        .split("ULL")
        .next()
        .expect("end")
        .to_string();

    let dir = tmp("aw");
    std::fs::create_dir_all(&dir).unwrap();
    let prof = dir.join("p.json");
    // `i` observed in bucket 1 (fits u8); the loop's counter and incr
    // stores must then be guarded against [0, 255].
    std::fs::write(
        &prof,
        format!(
            "{{\"schema\":1,\"hash_algo\":\"fnv1a64\",\"source_hash\":\"{hash}\",\
             \"options\":{{\"target\":\"c\"}},\"loops\":[],\"branches\":[],\
             \"mags\":[{{\"name\":\"main:mag:i@0\",\"max_bucket\":1}}],\"lens\":[]}}"
        ),
    )
    .unwrap();

    std::env::set_var("SH2_ASSUME_OBSERVED_WIDTHS", "1");
    std::env::set_var("SH2_PROFILE_IN", &prof);
    std::env::remove_var("SH2_PROFILE");
    std::env::remove_var("SH2_PROFILE_OUT");
    let c = {
        let prog = debashl::shir_json_in::shir_json_to_ir(&prog_json()).unwrap();
        debashl::c_backend::shir_to_c(&prog)
    };
    std::env::remove_var("SH2_ASSUME_OBSERVED_WIDTHS");
    std::env::remove_var("SH2_PROFILE_IN");

    assert!(
        c.contains("exceeded the observed range [0, 255]"),
        "assumed-width store guard missing:\n{c}"
    );
    // The narrowed declaration must exist (u8/i8 or wider via i+1's
    // range — assert only that it is no longer `long long`).
    assert!(
        !c.contains("long long i;") && !c.contains("long long i ="),
        "i was not narrowed:\n{c}"
    );
}

/// The middle tier: a bucket in 9..=16 means the observed value needs
/// 65..128 bits, so the var is homed in `__int128` (exact and ~10-15x
/// faster than GMP) instead of GMP. Requires the wide probe to have
/// been taken, since the i64 instrumented build would have wrapped.
#[test]
fn assume_observed_width_i128_tier() {
    let _g = TEST_LOCK.lock().unwrap();
    let pc = render_c(&prog_json(), true, &tmp("i128_prof.json"));
    let hash = pc
        .lines()
        .find(|l| l.contains("_prof_hash ="))
        .expect("hash line")
        .split("0x")
        .nth(1)
        .expect("hex")
        .split("ULL")
        .next()
        .expect("end")
        .to_string();

    let dir = tmp("i128");
    std::fs::create_dir_all(&dir).unwrap();
    let prof = dir.join("p.json");
    // bucket 13 = 2**100-ish: fits 128, not 64
    std::fs::write(
        &prof,
        format!(
            "{{\"schema\":1,\"hash_algo\":\"fnv1a64\",\"source_hash\":\"{hash}\",\
             \"options\":{{\"target\":\"c\"}},\"loops\":[],\"branches\":[],\
             \"mags\":[{{\"name\":\"main:mag:i@0\",\"max_bucket\":13}}],\"lens\":[]}}"
        ),
    )
    .unwrap();

    std::env::set_var("SH2_ASSUME_OBSERVED_WIDTHS", "1");
    std::env::set_var("SH2_PROFILE_IN", &prof);
    std::env::remove_var("SH2_PROFILE");
    std::env::remove_var("SH2_PROFILE_OUT");
    let c = {
        let prog = debashl::shir_json_in::shir_json_to_ir(&prog_json()).unwrap();
        debashl::c_backend::shir_to_c(&prog)
    };
    std::env::remove_var("SH2_ASSUME_OBSERVED_WIDTHS");
    std::env::remove_var("SH2_PROFILE_IN");

    assert!(
        c.contains("__int128 i = 0;"),
        "i was not homed in the __int128 middle tier:\n{c}"
    );
    assert!(
        c.contains("_sh_i128_str("),
        "__int128 value must print through _sh_i128_str:\n{c}"
    );
    assert!(
        !c.contains("mpz_t i;"),
        "bucket 13 must not fall through to GMP:\n{c}"
    );
}

/// A minimal grower (`x = x * 2` in a loop) for the tiered test.
fn grow_prog_json() -> String {
    r##"{
 "type": "Program", "contract_version": 1,
 "imports": [], "requires": [], "subs": [],
 "var_types": [{"name": "x", "type": "Int"}, {"name": "i", "type": "Int"}],
 "stmts": [
  {"type": "Assign", "targets": [{"var": "x", "sigil": null, "indices": []}],
   "expr": {"type": "Str", "style": "DoubleQuoted", "value": "1"}},
  {"type": "Assign", "targets": [{"var": "i", "sigil": null, "indices": []}],
   "expr": {"type": "Str", "style": "DoubleQuoted", "value": "0"}},
  {"type": "While", "runs": true,
   "cond": {"type": "Call", "func": "test", "purity": "Emulable",
    "args": [{"type": "Str", "style": "DoubleQuoted", "value": "$i -lt 5"}]},
   "body": [
    {"type": "Assign", "targets": [{"var": "x", "sigil": null, "indices": []}],
     "expr": {"type": "Arith", "ast": {"type": "Bin", "op": "*",
      "lhs": {"type": "Var", "name": "x"}, "rhs": {"type": "Num", "value": 2}}}},
    {"type": "Assign", "targets": [{"var": "i", "sigil": null, "indices": []}],
     "expr": {"type": "Arith", "ast": {"type": "Bin", "op": "+",
      "lhs": {"type": "Var", "name": "i"}, "rhs": {"type": "Num", "value": 1}}}}
   ]},
  {"type": "Expr", "expr": {"type": "Call", "func": "exec", "purity": "Emulable",
   "args": [{"type": "Str", "style": "DoubleQuoted", "value": "echo"},
    {"type": "Array", "elements": [{"type": "Call", "func": "getVar", "purity": "Emulable",
      "args": [{"type": "Str", "style": "DoubleQuoted", "value": "x"}]}]}]}}
 ]
}"##.to_string()
}

/// Bucket 17 (beyond 128 bits) on a detected grower must keep BOTH tiers:
/// an `__int128` fast mirror that spills to the GMP slot on overflow.
/// This is what makes a merged profile (max bucket 17) still contain the
/// fast 128-bit path instead of degrading to pure GMP.
#[test]
fn assume_observed_width_tiered_grower() {
    let _g = TEST_LOCK.lock().unwrap();
    let pc = render_c(&grow_prog_json(), true, &tmp("tier_prof.json"));
    let hash = pc
        .lines()
        .find(|l| l.contains("_prof_hash ="))
        .expect("hash line")
        .split("0x")
        .nth(1)
        .expect("hex")
        .split("ULL")
        .next()
        .expect("end")
        .to_string();

    let dir = tmp("tier");
    std::fs::create_dir_all(&dir).unwrap();
    let prof = dir.join("p.json");
    std::fs::write(
        &prof,
        format!(
            "{{\"schema\":1,\"hash_algo\":\"fnv1a64\",\"source_hash\":\"{hash}\",\
             \"options\":{{\"target\":\"c\"}},\"loops\":[],\"branches\":[],\
             \"mags\":[{{\"name\":\"main:mag:x@0\",\"max_bucket\":17}}],\"lens\":[]}}"
        ),
    )
    .unwrap();

    std::env::set_var("SH2_ASSUME_OBSERVED_WIDTHS", "1");
    std::env::set_var("SH2_PROFILE_IN", &prof);
    std::env::remove_var("SH2_PROFILE");
    std::env::remove_var("SH2_PROFILE_OUT");
    let c = {
        let prog = debashl::shir_json_in::shir_json_to_ir(&grow_prog_json()).unwrap();
        debashl::c_backend::shir_to_c(&prog)
    };
    std::env::remove_var("SH2_ASSUME_OBSERVED_WIDTHS");
    std::env::remove_var("SH2_PROFILE_IN");

    assert!(c.contains("__int128 x_f = 0;"), "fast mirror missing:\n{c}");
    assert!(c.contains("mpz_t x;"), "GMP spill slot missing:\n{c}");
    assert!(c.contains("int x_big = 0;"), "spill flag missing:\n{c}");
    assert!(
        c.contains("__builtin_mul_overflow(x_f"),
        "fast path overflow check missing:\n{c}"
    );
    assert!(
        c.contains("if (x_big) _sh_mpz_str"),
        "runtime-select print missing:\n{c}"
    );
}
