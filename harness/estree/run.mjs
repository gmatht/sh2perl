#!/usr/bin/env node
// run.mjs — ESTree JSON → JS converter entry (the sh2loop estree-gen.mjs
// converter: astring + lower.js passes — the same one fail-estree uses).
// Reads an ESTree JSON file, renders it to JS, prints the JS to stdout.
//
// Usage: node run.mjs <program.estree.json>
import fs from 'node:fs';
import { generate } from './estree-gen.mjs';

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
process.stdout.write(generate(program));
