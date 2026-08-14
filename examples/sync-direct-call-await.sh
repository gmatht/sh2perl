#!/bin/sh
# The A1 ESTree emitter types a function that only makes DIRECT calls
# as SYNC (non-async arrow) yet still emits `await` in front of the
# direct call when the callee is ASYNC — a SyntaxError ("await is only
# valid in async functions and the top level bodies of modules").
#
# The callee turns ASYNC because a stderr redirect (2>/dev/null) lowers
# to `await sh2.redirect(...)`. Without the redirect the same program
# transpiles cleanly (the direct call stays await-free).
#
# Transpiling this file to JS emits:
#   (__fn_g = async () => {
#     await sh2.redirect(() => sh2.builtin("date", []), [{
#       fd: 2, mode: "w", target: "/dev/null"
#     }]);
#   }, sh2.functions.set("g", __fn_g));
#   (__fn_f = () => {
#     await sh2.callDirect("g", __fn_g, []);   // await in a NON-async arrow
#   }, sh2.functions.set("f", __fn_f));
#
# Minimal form: the redirect is the only thing needed to trigger it;
# `echo hi 2>/dev/null` or `true 2>/dev/null` work just as well.
g() { date 2>/dev/null; }
f() { g; }
f
