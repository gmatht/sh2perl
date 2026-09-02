//! Stub for the legacy AST-side Perl generator (`src/generator/`) when the
//! `legacy-generator` cargo feature is OFF (the default).
//!
//! The real generator is 36k lines and ~600 KB of release `.text`; it is
//! only needed for (a) the new perl backend's command emulation
//! (`ir::generator_emulate_command` — which falls back to `bash -c`
//! shell-out when this feature is off) and (b) the legacy CLI / wasm / wasi
//! `to_perl` utilities. This stub keeps every consumer compiling with the
//! feature off; the legacy commands degrade to a clear message instead of
//! generating Perl.

/// Minimal stand-in for `generator::Generator`. Every method the consumers
/// call exists; `generate`/`word_to_perl` return a Perl comment explaining
/// that the feature is off.
pub struct Generator {
    pub use_function_signatures: bool,
}

impl Generator {
    pub fn new() -> Self {
        Self {
            use_function_signatures: false,
        }
    }
    pub fn new_inline_mode() -> Self {
        Self::new()
    }
    pub fn generate(&self, _commands: &[crate::ast::Command]) -> String {
        "# perl generation requires the `legacy-generator` feature (cargo build --features legacy-generator)"
            .to_string()
    }
    pub fn word_to_perl(&self, _w: &crate::ast::Word) -> String {
        self.generate(&[])
    }
    pub fn perl_string_literal_no_interp(&self, _w: &crate::ast::Word) -> String {
        String::new()
    }
    pub fn set_original_script_name(&mut self, _name: String) {}
}
