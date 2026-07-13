//! Build script: make `cargo build` rebuild the server whenever the embedded
//! widget assets change.
//!
//! `rust-embed` reads `packages/turnstile-widget/dist/` at compile time via a
//! derive macro, but cargo doesn't automatically re-run the build when the
//! folder's *contents* change — so a widget rebuild wouldn't be picked up
//! without this. We walk `dist/` and emit `rerun-if-changed` for the directory
//! and every file in it.

use std::path::Path;

fn main() {
    let dist = Path::new("../../packages/turnstile-widget/dist");
    // Track the directory itself (creation / removal).
    println!("cargo:rerun-if-changed={}", dist.display());
    if dist.exists() {
        walk(dist);
    } else {
        println!(
            "cargo:warning=turnstile-widget dist/ not found; \
             run `just build-widget` before building the server"
        );
    }
}

fn walk(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        println!("cargo:rerun-if-changed={}", path.display());
        if path.is_dir() {
            walk(&path);
        }
    }
}
