#[test]
fn probe_let_shapes() {
    use debashl::parser::commands::parse_commands_from_text;
    use debashl::shir::ast_to_ir_raw;
    use debashl::shir_json::shir_to_shir_json;
    for src in ["let \"x=1\"", "let \"x+=1\"", "let \"x++\"", "let \"x\""] {
        let cmds = parse_commands_from_text(src).expect("parse");
        let prog = ast_to_ir_raw(&cmds);
        eprintln!("=== {src} ===\n{}", shir_to_shir_json(&prog));
    }
}

/// TEMPORARY probe (delete after use): numstr verdicts for the game's
/// hot rasterizer vars/params.
#[test]
fn probe_numstr_mimecroft() {
    use debashl::shir_json_in::shir_json_to_ir;
    use debashl::shir_passes::numstr;
    let s = std::fs::read_to_string("/tmp/perf/game.shir").expect("shir");
    let prog = shir_json_to_ir(&s).expect("json");
    let empty = std::collections::BTreeSet::new();
    let int_arrays = debashl::shir::analyze_int_arrays(&prog);
    let inputs = numstr::Inputs { blocked_extra: &empty, int_arrays: &int_arrays };
    let v = numstr::seed(&prog, &inputs);
    eprintln!("dyn_call={} ints={} params_fns={} blocked={} int_arrays={}",
        v.dyn_call, v.ints.len(), v.params.len(), v.blocked.len(), int_arrays.len());
    for name in ["rbp_y","rbp_n","rbp_x0","rbp_row","rbt_y","rbt_runx",
                 "rbt_runlen","rbt_apx","rbt_ox","rbv_s","rbv_v","rbc_ox",
                 "rbt_wx","rbt_wy","rbt_wz"] {
        let asg = v.assigns.get(name).map(|a| a.iter()
            .map(|(f,e)| format!("{f}:{}", numstr::shape(e)))
            .collect::<Vec<_>>().join(" | ")).unwrap_or_default();
        eprintln!("VAR {name}: int={} blocked={} pinned={} assigns=[{asg}]",
            v.ints.contains(name), v.blocked.contains(name), v.pinned.contains(name));
    }
    for f in ["rb_putrun","rb_tv","rb_cube","rb_tri","rb_frag","rb_project","rb_mkrun"] {
        eprintln!("FN {f}: params={:?} sites={}", v.params.get(f),
            v.sites.get(f).map(|s| s.len()).unwrap_or(0));
        if let Some(ss) = v.sites.get(f) {
            for (caller, args) in ss.iter().take(8) {
                let shapes: Vec<String> = args.iter().map(|a| numstr::shape(a)).collect();
                eprintln!("    from {caller}: [{}]", shapes.join(", "));
            }
        }
    }
}

/// TEMPORARY: recursively locate the first non-int subexpression.
#[test]
fn probe_numstr_why() {
    use debashl::ir::IrExpr;
    use debashl::shir_json_in::shir_json_to_ir;
    use debashl::shir_passes::numstr;
    fn why(e: &IrExpr, ints: &std::collections::HashSet<String>,
           params: &std::collections::HashMap<String, std::collections::BTreeSet<usize>>,
           cur: &str, ia: &std::collections::HashSet<String>, depth: usize) -> String {
        let shape = numstr::shape(e);
        if numstr::expr_int(e, ints, params, cur, ia) { return format!("{shape}=INT"); }
        if depth > 6 { return format!("{shape}=NO(deep)"); }
        let inner = match e {
            IrExpr::Call { func, args } => format!("{func}(") + &args.iter()
                .map(|a| why(a, ints, params, cur, ia, depth + 1))
                .collect::<Vec<_>>().join(", ") + ")",
            IrExpr::BinOp { lhs, rhs, .. } => format!("{} , {}",
                why(lhs, ints, params, cur, ia, depth + 1),
                why(rhs, ints, params, cur, ia, depth + 1)),
            IrExpr::Interpolate(parts) => format!("{parts:?}"),
            IrExpr::Index { key, .. } => why(key, ints, params, cur, ia, depth + 1),
            IrExpr::Var(n, _) | IrExpr::Ident(n) => format!("var({n})"),
            IrExpr::Str(t, _) => format!("lit({t})"),
            _ => String::new(),
        };
        format!("{shape}=NO[{inner}]")
    }
    let s = std::fs::read_to_string("/tmp/perf/game.shir").expect("shir");
    let prog = shir_json_to_ir(&s).expect("json");
    let empty = std::collections::BTreeSet::new();
    let int_arrays = debashl::shir::analyze_int_arrays(&prog);
    let inputs = numstr::Inputs { blocked_extra: &empty, int_arrays: &int_arrays };
    let v = numstr::seed(&prog, &inputs);
    for name in ["rbt_miny","rbt_minx","rbt_maxx","rbt_maxy","rbov_by","rbov_bx","rbt_dy","rbt_dx","rbt_wx","rbt_dz","rbf_pj0","rbf_p0"] {
        let Some(als) = v.assigns.get(name) else { eprintln!("VAR {name}: NO ASSIGNS"); continue; };
        eprintln!("VAR {name}: int={} blocked={}", v.ints.contains(name), v.blocked.contains(name));
        for (f, e) in als {
            if !numstr::expr_int(e, &v.ints, &v.params, f, &int_arrays) {
                eprintln!("    FAIL in {f}: {}", why(e, &v.ints, &v.params, f, &int_arrays, 0));
            }
        }
    }
}

/// TEMPORARY: which call-site actuals block the rasterizer param legs?
#[test]
fn probe_numstr_sites() {
    use debashl::ir::IrExpr;
    use debashl::shir_json_in::shir_json_to_ir;
    use debashl::shir_passes::numstr;
    fn why(e: &IrExpr, ints: &std::collections::HashSet<String>,
           params: &std::collections::HashMap<String, std::collections::BTreeSet<usize>>,
           cur: &str, ia: &std::collections::HashSet<String>, depth: usize) -> String {
        if numstr::expr_int(e, ints, params, cur, ia) { return "INT".into(); }
        if depth > 4 { return "NO(deep)".into(); }
        match e {
            IrExpr::Call { func, args } => {
                let inner: Vec<String> = args.iter()
                    .map(|a| why(a, ints, params, cur, ia, depth + 1)).collect();
                format!("{func}({})", inner.join(","))
            }
            IrExpr::Var(n, _) | IrExpr::Ident(n) => format!("var({n})"),
            IrExpr::Str(t, _) => format!("lit({t})"),
            _ => numstr::shape(e),
        }
    }
    let s = std::fs::read_to_string("/tmp/perf/game.shir").expect("shir");
    let prog = shir_json_to_ir(&s).expect("json");
    let empty = std::collections::BTreeSet::new();
    let int_arrays = debashl::shir::analyze_int_arrays(&prog);
    let inputs = numstr::Inputs { blocked_extra: &empty, int_arrays: &int_arrays };
    let v = numstr::seed(&prog, &inputs);
    for f in ["rb_cube","rb_putrun","rb_tv","rb_project"] {
        let Some(ss) = v.sites.get(f) else { continue };
        for (caller, args) in ss {
            for (i, a) in args.iter().enumerate() {
                if !numstr::expr_int(a, &v.ints, &v.params, caller, &int_arrays) {
                    eprintln!("{f}[{i}] from {caller}: {}", why(a, &v.ints, &v.params, caller, &int_arrays, 0));
                }
            }
        }
    }
}
