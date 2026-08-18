# sh2perl (debashc) — Shell Script to Perl Converter

[![Tests](https://github.com/gmatht/sh2perl/actions/workflows/test.yml/badge.svg)](https://github.com/gmatht/sh2perl/actions/workflows/test.yml)

<!-- Dynamic badges for test counts served from gh-pages branch -->
[![Purify tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/gmatht/sh2perl/gh-pages/.github/badges/purify.json)](https://github.com/gmatht/sh2perl/actions/workflows/test.yml)
[![Main tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/gmatht/sh2perl/gh-pages/.github/badges/main-tests.json)](https://github.com/gmatht/sh2perl/actions/workflows/test.yml)

A Rust library and command-line tool that parses shell/bash scripts and converts
them to equivalent Perl. The goal is behavioral fidelity: the generated Perl
should produce the same stdout as `LANG=C bash` would on Linux/WSL. A corpus of
530 example scripts is used to verify this, comparing the output of each script
under bash against the output of its Perl translation.

**🌐 [Try the Live Demo](https://dansted.org/Debashc8/) — convert shell scripts in your browser (WebAssembly build).**

The binary is currently named `debashc`. If you see references to `sh2perl`,
that is the repository/project name; in the future there may be separate
`sh2perl`, `sh2rust`, etc. front-ends.

## Features

- **Complete lexer** — tokenizes shell/bash scripts, including nested quoting,
  command substitution, here-documents, and ANSI-C quoting
- **AST parser** — converts tokens into a structured Abstract Syntax Tree
- **Perl code generation** — produces standalone Perl programs
  (`use strict; use warnings;` clean, checked with Perl::Critic)
- **Intermediate representations** — can export language-neutral ShIR and MIR
  as JSON, and standard ESTree JSON, for building other back-ends
- **WebAssembly support** — run the converter in the browser
- **Shell constructs supported**: pipelines and redirections, control flow
  (if/elif/else, for, while/until, case, select), functions, variable and
  parameter expansions, command substitution, arithmetic expansion, arrays
  (indexed and associative), here-documents/here-strings, process substitution,
  file test operators, and many common external commands (grep, awk, sed, sort,
  wc, find, ls, …) translated to native Perl

## Why Not Use an LLM Instead?

While Large Language Models can translate shell scripts, this specialized
transcoder offers:

- **Deterministic output** — every conversion produces identical, predictable
  results; no hallucinated functions or syntax
- **Speed and cost** — converts scripts in milliseconds, offline, with no API
  fees or rate limits
- **Verified fidelity** — every change is checked against a 530-script corpus
  comparing bash output with the generated Perl's output
- **CI/CD ready** — a library API and CLI that can be embedded in build
  pipelines and batch workflows

## Installation

```bash
git clone https://github.com/gmatht/sh2perl.git
cd sh2perl
cargo build --release
```

See [INSTALL.md](INSTALL.md) for dependency details (Perl modules used by the
test harness) and [DOCKER.md](DOCKER.md) for a containerized environment.

## Usage

### Command Line Interface

```bash
# Tokenize a shell script
debashc lex 'echo hello world'

# Parse a shell script to AST
debashc parse 'ls | grep test'

# Convert a shell script to Perl
debashc parse --perl 'ls | grep test'

# Convert a shell script file to Perl
debashc file --perl examples/001_echo_basic.sh

# Export the language-neutral ShIR as JSON
debashc file --shir examples/001_echo_basic.sh

# Generate and run the Perl translation
debashc file --run perl examples/001_echo_basic.sh

# Interactive mode
debashc interactive
```

Run `debashc --help` for the full command list.

### Testing commands

```bash
# Run one corpus test (compares bash output vs generated-Perl output)
debashc --test-file perl examples/001_echo_basic.sh

# Run the whole corpus, stopping at each failure
debashc --next-fail

# Run the corpus starting from the first failing test, with Perl::Critic checks
debashc fail --perl-critic

# Clear the cached outputs used by the test harness
debashc --clear-cache
```

### Library Usage

```rust
use debashl::{Lexer, Parser, Generator};

let input = "echo hello world";

// Parse a shell script
let mut parser = Parser::new(input);
let commands = parser.parse().expect("parse error");

// Convert to Perl
let mut generator = Generator::new();
let perl_code = generator.generate(&commands);
println!("{}", perl_code);
```

## Web Interface

A WebAssembly build powers an in-browser converter.

```bash
# Build the WASM target
./scripts/build-wasm.sh

# Or manually:
cargo install wasm-pack
wasm-pack build --target web --out-dir www/pkg

# Serve the web interface
cd www
python3 -m http.server 8000
# Then open http://localhost:8000 in your browser
```

See [docs/WASM.md](docs/WASM.md) for the Windows PowerShell workflow and
`scripts/build-wasi.sh` for the WASI build.

## Example Conversion

```bash
debashc parse --perl 'if [ -f file.txt ]; then echo "File exists"; fi'
```

Output (excerpt):

```perl
if (-f 'file.txt') {
    print "File exists\n";
}
```

## Project Structure

```
sh2perl/
├── src/                # The transpiler library (crate `debashl`)
│   ├── lexer.rs        # Tokenizer
│   ├── parser/         # Token stream → AST
│   ├── ast.rs          # AST definitions
│   ├── generator/      # AST → Perl code generation
│   ├── ir.rs           # Perl-oriented IR helpers
│   ├── shir.rs         # Language-neutral ShIR (JSON export)
│   ├── estree.rs       # ESTree JSON export
│   └── bin/            # Debugging utilities (token/AST dumpers)
├── cli/                # The `debashc` command-line tool (crate `debashcl`)
├── examples/           # The 530-script test corpus
├── examples.impurl/    # Inputs for the purify.pl test suite
├── docs/               # Design notes and developer documentation
├── scripts/            # Build, benchmark, and maintenance scripts
├── ideom/              # Idiom reviews of generated code
├── www/                # Web interface for the WASM build
├── purify.pl           # Post-processor that cleans generated Perl
├── test_purify.pl      # Test suite for purify.pl (run by CI)
└── Cargo.toml          # Workspace configuration
```

## Testing

```bash
# Rust unit/integration tests
cargo test

# Purify test suite
perl test_purify.pl --all

# Full corpus equivalence run
./target/debug/debashc --next-fail
```

The corpus runner compares stdout of each `examples/*.sh` under bash against
the generated Perl's stdout. Cached outputs are stored in
`command_cache.json` and generated Perl in `examples.out/` (both are local
artifacts, ignored by git).

## Creating Good Examples

When contributing examples to the corpus:

1. **Test specific features** — focus on one or two shell constructs per
   example, including edge cases.
2. **Use clear names** — `001_simple.sh`, `030_arrays_associative.sh`, …
3. **Add validation comments** where useful:
   - `#PERL_MUST_CONTAIN: pattern` — generated Perl must contain this pattern
   - `#PERL_MUST_NOT_CONTAIN: pattern` — generated Perl must NOT contain it
   - `#AST_MUST_CONTAIN:` / `#AST_MUST_NOT_CONTAIN:` — same for the AST dump
4. **Make examples self-contained** — create and clean up any files they need;
   avoid depending on external state.
5. **Produce meaningful output** — the corpus compares stdout, so silent
   operations are not tested.
6. **Run the tests locally** before submitting:
   ```bash
   ./target/debug/debashc --test-file perl examples/your_example.sh
   ```

## Documentation

- [docs/AST.md](docs/AST.md) — AST reference
- [docs/ir-design.md](docs/ir-design.md) — IR design notes
- [docs/backend-universal-contract.md](docs/backend-universal-contract.md) — contract for additional back-ends
- [docs/ideom-workflow.md](docs/ideom-workflow.md) — idiom review workflow
- [docs/BENCHMARKS.md](docs/BENCHMARKS.md) — benchmarking the generated code
- [docs/WASM.md](docs/WASM.md) — WebAssembly build details
- [docs/TRANSLATIONS.md](docs/TRANSLATIONS.md) — catalog of shell→Perl translations
- [AGENTS.md](AGENTS.md) — ground rules for AI-assisted development in this repo

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes and add tests/examples for new functionality
4. Run `cargo test` and the corpus (`./target/debug/debashc --next-fail`)
5. Submit a pull request

## Roadmap

- [ ] Support for more shell features and builtins
- [ ] Additional target languages via the ShIR/ESTree back-end contract
- [ ] Get the remaining corpus tests passing

## License

This project is licensed under the GPLv3 License — see the [LICENSE](LICENSE)
file for details.
