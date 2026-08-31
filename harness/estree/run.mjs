#!/usr/bin/env node
// run.mjs — ESTree JSON → JS converter entry (the sh2runtime estree.js
// converter, vendored). Reads an ESTree JSON file, renders it to JS via
// estreeToJs (estree.js + astring + lower.js), prints the JS to stdout.
//
// Usage: node run.mjs <program.estree.json>
import fs from 'node:fs';
import { estreeToJs } from './estree.js';

const jsonPath = process.argv[2];
if (!jsonPath) {
  console.error('usage: node run.mjs <program.estree.json>');
  process.exit(2);
}
let program;
try {
  program = JSON.parse(fs.readFileSync(jsonPath, 'utf8'));
} catch (e) {
  console.error(`run.mjs: cannot read/parse ${jsonPath}: ${e.message}`);
  process.exit(2);
}
const js = await estreeToJs(program);
process.stdout.write(js);
