# WASM Build and Run Script

This PowerShell script (`build-and-run-wasm.ps1`) automates the process of building and running the WASM version of the debashc project.

## Prerequisites

- **Rust**: Make sure Rust is installed on your system
- **Python**: Required for the HTTP server (Python 3.x recommended)
- **PowerShell**: The script runs on PowerShell 5.1 or later

## Usage

### Basic Usage (Default Browser)
```powershell
.\build-and-run-wasm.ps1
```

### Specify Browser
```powershell
# Use Edge
.\build-and-run-wasm.ps1 -Browser edge

# Use Chrome
.\build-and-run-wasm.ps1 -Browser chrome

# Use default browser
.\build-and-run-wasm.ps1 -Browser default
```

### Specify Port
```powershell
# Use a different port (default is 8000)
.\build-and-run-wasm.ps1 -Port 8080
```

### Combined Parameters
```powershell
.\build-and-run-wasm.ps1 -Browser edge -Port 8080
```

## What the Script Does

1. **Checks Prerequisites**: Verifies that `wasm-pack` is installed
2. **Installs wasm-pack**: Automatically installs `wasm-pack` if not found
3. **Builds WASM**: Compiles the Rust project to WebAssembly
4. **Starts HTTP Server**: Launches a Python HTTP server in the `www` directory
5. **Opens Browser**: Automatically opens the specified browser to `http://localhost:8000`

## Features

- **Automatic Installation**: Installs `wasm-pack` if not present
- **Error Handling**: Comprehensive error checking and reporting
- **Multiple Browser Support**: Edge, Chrome, or default browser
- **Configurable Port**: Can specify custom port numbers
- **Colored Output**: Clear, colored console output for better UX
- **Background Server**: HTTP server runs in background

## Stopping the Server

To stop the HTTP server, press `Ctrl+C` in the PowerShell window where the script is running.

## Troubleshooting

### Common Issues

1. **Python not found**: Install Python from [python.org](https://python.org)
2. **Rust not found**: Install Rust from [rustup.rs](https://rustup.rs)
3. **Port already in use**: Use a different port with the `-Port` parameter
4. **Browser not opening**: Check if the specified browser is installed

### Manual Steps

If the script fails, you can run the steps manually:

```powershell
# Install wasm-pack
cargo install wasm-pack

# Build WASM
wasm-pack build --target web --out-dir www/pkg

# Start server (in www directory)
cd www
python -m http.server 8000

# Open browser manually to http://localhost:8000
```

## File Structure

After running the script, the following structure will be created:

```
sh2perl/
├── build-and-run-wasm.ps1    # This script
├── www/
│   ├── index.html            # Main application page
│   └── pkg/                  # Generated WASM files
│       ├── debashc.js
│       ├── debashc_bg.wasm
│       └── ...
└── ...
```

## Browser Compatibility

The WASM application works best in modern browsers:
- Microsoft Edge (recommended)
- Google Chrome
- Firefox
- Safari (macOS)

## Development

To modify the script:
1. Edit `build-and-run-wasm.ps1`
2. Test changes by running the script
3. The script includes detailed comments for easy modification

## WASI (server-side) build

`build-wasi.sh` produces two artifacts for the `wasm32-wasip1` target, sharing
the same parsing/transpiling code:

| Artifact | Role | Entry | Use |
|---|---|---|---|
| `target/wasm32-wasip1/release/debashc.wasm` | WASI **command** | `_start` | `wasmtime run --dir . debashc.wasm file --perl script.sh` (also `file --estree`, `parse`, `lex`, ...) |
| `target/wasm32-wasip1/release/debashl.wasm` | WASM **library** | `_initialize` | instantiate and call `debashc_to_perl` / `debashc_to_estree` / `debashc_lex` / `debashc_version` (plain C ABI, no JS glue) |

Build with:

```bash
./build-wasi.sh
```

A single module *can* carry both `_start` and library exports, but strict
runtimes (e.g. Node `node:wasi`) reject a module exporting both `_start` and
`_initialize`, so the command and library builds stay separate
(`wasi-lib` cargo feature, see `src/wasi_api.rs` for the memory contract and
JSON result envelope). The library is callable from wasmtime/wasmer embedding
APIs, Node `node:wasi`, Python `wasmtime`, C/C++, etc. — no wasm-bindgen, no
browser glue. `debashc file --estree` / `debashc_to_estree` emit the standard
ESTree JSON contract (PLAN.md §1.2), so sh2runtime can consume this WASI
binary as a transpiler tool.
