// estree-gen.mjs — ESTree JSON → JS text (astring generator + lower.js passes).
//
// Replaces the original hand-rolled printer with the sh2runtime estree→JS
// translation: astring (vendored at vendor/astring.mjs) renders the tree,
// and the shared optimization pipeline in lower.js (copied from
// sh2runtime/src/lower.js) rewrites provably-safe shapes before printing.
//
// The sh2perl emitter produces a FIXED vocabulary of standard ESTree nodes
// (src/estree.rs). astring handles every node type astring knows (no
// "unsupported statement" gaps), including ForStatement/ForOfStatement/
// SequenceExpression and the sh2.* runtime calls; an unknown node type
// still throws, so the harness fails loudly if the emitter grows a
// construct the backend cannot print.
//
// Output is used only as an intermediate: the reference executor runs it
// under node with the `sh2` namespace global (see estree-runner.mjs).

import { generate as astringGenerate } from './astring.mjs';
import {
  hoistLoopLastExit, hoistCommonLastExit,
  dropDeadFlags, mergeInitAssignments,
} from './lower.js';

// ── sh2loop-specific pre-print rewrites ────────────────────────────
// (a) the A1 `split` marker (unquoted expansions — exec args, for-iters)
// is emitted by the core as a NATIVE inline `String(x).split(/\s+/)
// .filter(w => w.length > 0)`. Dispatch it through `sh2.split` instead:
// identical for bash semantics (sh2.split implements the same split), but
// lets the runtime resolve SOURCE-LANGUAGE differences at the execution
// boundary — zsh never field-splits unquoted expansions (SH_WORD_SPLIT off
// by default), so in zsh mode sh2.split returns the whole value as a
// single field (the _setLang mechanism, same as 1-based arrays).
// (b) regex literals (`/\s+/`, the case-glob lowering): a literal regex
// cannot contain raw line terminators (newline/CR/U+2028/U+2029) — glob
// patterns may embed real newlines (`case $x in *'\n'*`), so escape them.
// astring prints node.regex verbatim, so this is done on the tree first.

// splitInlineArg — recognize the emitter's native inline field-split shape
// and return the inner `x` expression; null for anything else.
function splitInlineArg(n) {
  if (n.type !== 'CallExpression') return null;
  const callee = n.callee;
  if (
    callee?.type !== 'MemberExpression' || callee.computed ||
    callee.property?.type !== 'Identifier' || callee.property.name !== 'filter' ||
    n.arguments.length !== 1 || n.arguments[0].type !== 'ArrowFunctionExpression'
  ) return null;
  const arrow = n.arguments[0];
  if (
    arrow.params.length !== 1 || arrow.params[0].type !== 'Identifier' ||
    arrow.body?.type !== 'BinaryExpression' || arrow.body.operator !== '>' ||
    arrow.body.right?.type !== 'Literal' || arrow.body.right.value !== 0 ||
    arrow.body.left?.type !== 'MemberExpression' ||
    arrow.body.left.object?.type !== 'Identifier' ||
    arrow.body.left.object.name !== arrow.params[0].name ||
    arrow.body.left.property?.type !== 'Identifier' ||
    arrow.body.left.property.name !== 'length'
  ) return null;
  const splitCall = callee.object;
  if (
    splitCall?.type !== 'CallExpression' || splitCall.arguments.length !== 1 ||
    splitCall.arguments[0]?.type !== 'Literal' ||
    splitCall.arguments[0].regex?.pattern !== '\\s+' ||
    splitCall.callee?.type !== 'MemberExpression' || splitCall.callee.computed ||
    splitCall.callee.property?.type !== 'Identifier' || splitCall.callee.property.name !== 'split'
  ) return null;
  const strCall = splitCall.callee.object;
  if (
    strCall?.type !== 'CallExpression' || strCall.arguments.length !== 1 ||
    strCall.callee?.type !== 'Identifier' || strCall.callee.name !== 'String'
  ) return null;
  return strCall.arguments[0];
}

// deep-clone + rewrite: split chains → sh2.split(x)
function rewriteSplitCalls(node) {
  if (!node || typeof node !== 'object') return node;
  if (Array.isArray(node)) return node.map(rewriteSplitCalls);
  const out = { ...node };
  for (const k of Object.keys(node)) {
    if (k === 'loc') continue;
    out[k] = rewriteSplitCalls(node[k]);
  }
  if (out.type === 'CallExpression') {
    const inner = splitInlineArg(out);
    if (inner !== null) {
      return {
        type: 'CallExpression',
        callee: {
          type: 'MemberExpression',
          object: { type: 'Identifier', name: 'sh2' },
          property: { type: 'Identifier', name: 'split' },
          computed: false, optional: false,
        },
        arguments: [inner],
        optional: false,
      };
    }
  }
  return out;
}

// regex literal patterns must not contain raw line terminators — escape
function escapeRegexLines(node) {
  if (!node || typeof node !== 'object') return node;
  if (Array.isArray(node)) { node.forEach(escapeRegexLines); return node; }
  if (node.type === 'Literal' && node.regex) {
    node.regex = {
      pattern: node.regex.pattern
        .replace(/\r/g, '\\r')
        .replace(/\n/g, '\\n')
        .replace(/\u2028/g, '\\u2028')
        .replace(/\u2029/g, '\\u2029'),
      flags: node.regex.flags,
    };
  }
  for (const k of Object.keys(node)) {
    if (k === 'loc') continue;
    escapeRegexLines(node[k]);
  }
  return node;
}

/** Render a full Program to JS module text. */
export function generate(program) {
  if (!program || program.type !== 'Program') {
    throw new Error(`estree-gen: expected Program, got ${program && program.type}`);
  }
  // (1) sh2loop-specific AST rewrites (sh2.split dispatch, regex hygiene)
  let tree = rewriteSplitCalls(program);
  escapeRegexLines(tree);
  // (2) the shared lower.js optimization pipeline (from sh2runtime
  // estree.js). Which passes are SAFE for the sh2loop emitter's shapes:
  //   • hoistLoopLastExit / hoistCommonLastExit — fire only on `(cmd,
  //     sh2.lastExit = <number literal>, flag)` sequences; the sh2loop
  //     emitter's status records are `sh2.lastExit = <conditional>` inside
  //     sequences or 2-element `(cmd, lastExit = N)` forms → no-op.
  //   • dropDeadFlags — pops only PURE trailing elements (sh2loop's
  //     statement-value flag `sh2._g` is a member read → untouched);
  //     unwraps 1-element sequences, drops literal-only statements.
  //   • mergeInitAssignments — conservative purity/read guards; folds only
  //     `let v = D; v = <pure>;` with nothing observing the default.
  // Deliberately NOT applied (would corrupt sh2loop semantics):
  //   • pushLastExitToEnd — a standalone `sh2.lastExit = N;` is a SEQUENCED
  //     statement-status record here (`local ec=$?` reads the position);
  //     moving it would clobber `$?` right after a command.
  //   • lowerNativeArrays — its ref vocabulary (getVar/param slice) does not
  //     cover the sh2loop emitter's array reads (sh2.arrayIndex / arrayItems),
  //     so a setArray → native `let a = [...]` fold would orphan those
  //     runtime reads (empty store). The sh2loop emitter already emits
  //     native arrays itself when provable.
  // The sh2runtime frontend rewrites (stripProcessEnv, nullSentinel,
  // unwrapStoreString, returnInLoop, awaitSyncFnCalls, normalizeFunctions)
  // are otranspilerl/C-frontend-specific and are NOT part of this backend.
  hoistLoopLastExit(tree);
  hoistCommonLastExit(tree);
  dropDeadFlags(tree);
  mergeInitAssignments(tree);
  // (3) astring prints
  return astringGenerate(tree);
}
