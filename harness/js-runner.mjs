// js-runner.mjs — run a generated JS program (mydat's otranspiler js
// target, the ESTree->JS path) against the sh2.* runtime, comparing to bash.
//
// Usage: node js-runner.mjs <program.js> [--source file.sh] [--ns sh2-namespace.mjs]
//
// Reads the generated JS text, wraps it with the sh2 runtime (import +
// _init + _setAllowlist + _finish), and runs it under node. The program's
// stdout goes to this runner's stdout (inherit), so the harness captures it
// directly. Exits nonzero on runtime errors / failed `exit` builtin / timeouts.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';

const argv = process.argv.slice(2);
const jsPath = argv[0];
let name = null;
let sourceFile = null;
let nsPath = null;
const positional = [];
for (let i = 1; i < argv.length; i++) {
  if (argv[i] === '--name') name = argv[++i];
  else if (argv[i] === '--source') sourceFile = argv[++i];
  else if (argv[i] === '--ns') nsPath = argv[++i];
  else if (argv[i] === '--args') { positional.push(...argv.slice(i + 1)); break; }
}
if (!jsPath) {
  console.error('usage: node js-runner.mjs <program.js> [--source file.sh] [--ns sh2-namespace.mjs]');
  process.exit(2);
}
if (!nsPath) nsPath = path.join(import.meta.dirname, 'sh2-namespace.mjs');
if (!name && sourceFile) name = sourceFile;
if (!name) name = path.basename(jsPath);

let js;
try {
  js = fs.readFileSync(jsPath, 'utf8');
} catch (e) {
  console.error(`js-runner: cannot read ${jsPath}: ${e.message}`);
  process.exit(2);
}

const ns = await import(pathToFileURL(nsPath).href);
const BUILTIN_NAMES = ns.BUILTIN_NAMES || [];

// Security allowlist (same as estree-runner): every WORD token in the source
// .sh, minus shell builtins. The generated program may only spawn external
// binaries whose names appear in the source text.
const builtins = new Set(BUILTIN_NAMES);
let allowlist = null;
if (sourceFile) {
  try {
    const src = fs.readFileSync(sourceFile, 'utf8');
    allowlist = [...new Set(
      src.split(/[^A-Za-z0-9_.+@\/:.-]+/).filter(w => w.length > 0 && !builtins.has(w)),
    )];
  } catch { /* fall through to empty */ }
}
if (!allowlist) allowlist = [];

const moduleSrc =
  `import { sh2 } from ${JSON.stringify(nsPath)};\n` +
  `sh2._init(${JSON.stringify(name)}, ${JSON.stringify(positional)});\n` +
  `sh2._setAllowlist(${JSON.stringify([...new Set(allowlist)])});\n` +
  js +
  `\nawait sh2._finish();\n`;

// Scratch dir under os.tmpdir() (NOT the workspace — corpus tests that walk
// the tree would see it and reorder readdir layout).
const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'sh2js-run-'));
const modFile = path.join(tmpDir, 'prog.mjs');
fs.writeFileSync(modFile, moduleSrc);

try {
  execFileSync(process.execPath, [modFile], { stdio: 'inherit' });
} catch (e) {
  process.exit(e.status ?? 1);
} finally {
  fs.rmSync(tmpDir, { recursive: true, force: true });
}
