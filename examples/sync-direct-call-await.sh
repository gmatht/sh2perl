#!/bin/sh
# WHY THIS EXAMPLE WAS ADDED (2026-08-14): while debugging the
# mimecroft 3D-game render (a fractional-camera culling fix), the
# game intermittently failed to transpile with "await is only valid in
# async functions" — the game's start_anim() calls gtick(), whose body
# contains `date ... 2>/dev/null`; the A1 emitter typed start_anim as
# sync yet emitted `await` on the direct call. The game worked around
# it by inlining the clock read. This file is the distilled regression
# case: keep it green so the emitter fix (sync-typed caller + async
# direct-call callee) has a standing test.
#
# The bug: the A1 ESTree emitter types a function that only makes
# DIRECT calls as SYNC (non-async arrow) yet still emits `await` in
# front of the direct call when the callee is ASYNC — a SyntaxError
# ("await is only valid in async functions and the top level bodies of
# modules").
#
# The callee turns ASYNC because a stderr redirect (2>/dev/null) lowers
# to `await sh2.redirect(...)`. Without the redirect the same program
# transpiles cleanly (the direct call stays await-free).
#
# 2026-08-15 fix: the ORIGINAL example used `date` as the callee body,
# making the native output time-dependent (the current time changes
# every run — the gate could never match). `echo hi 2>/dev/null` keeps
# the identical emitter trigger (the stderr redirect is the only thing
# that makes the callee async) with DETERMINISTIC output — a red pin
# is only useful if it fails for the right reason.
g() { echo hi 2>/dev/null; }
f() { g; }
f
