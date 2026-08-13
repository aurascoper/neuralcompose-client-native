//! Emits an rpath so this binary finds llama.cpp's shared libraries.
//!
//! Byte-for-byte the same job as `crates/neuralcompose-headless/build.rs`, and
//! it exists here for the same reason its header gives: `cargo:rustc-link-arg`
//! applies only to targets of the package that emits it, so the rpath
//! `neuralcompose-llama` emits is not inherited by binaries depending on it.
//! Link *libraries* propagate through the dependency graph; link *arguments* do
//! not.
//!
//! Confirmed the hard way on 2026-08-13: without this the binary linked fine,
//! reported no stub warning, and `ldd` showed `libllama.so.0 => not found`. The
//! failure is at load time, not build time — which is exactly the run that
//! produces acceptance evidence.
//!
//! Reads the same environment variables as the llama crate and does nothing
//! when they are unset; a stub build has no shared library to find.

use std::env;
use std::path::PathBuf;

/// The commit this binary was built from, for the turn log's provenance
/// envelope (ADR-004) — or `None`.
///
/// Read from git at build time for the reason `neuralcompose-llama/build.rs`
/// gives about `NC_LLAMA_COMMIT`: a hash read from the working tree cannot
/// drift from the artefact, while one copied into a document can.
///
/// **A dirty tree yields `None`.** A commit alone does not describe a build with
/// uncommitted changes in it, and the envelope's contract is that a present
/// `gitCommit` means a reproducible build. Reporting HEAD here would be the
/// pinned-build claim without the pinning.
///
/// One `git status --porcelain=v2 --branch` call, not a `rev-parse` plus a
/// separate dirty check: two commands can straddle a commit and describe a
/// state that never existed.
fn git_commit() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["status", "--porcelain=v2", "--branch"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let mut oid = None;
    for line in text.lines() {
        match line.strip_prefix("# branch.oid ") {
            Some(v) => oid = Some(v.trim().to_string()),
            // Any non-header line is a changed, staged or untracked path.
            None if !line.starts_with('#') => return None,
            None => {}
        }
    }
    oid.filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn main() {
    println!("cargo:rerun-if-env-changed=LLAMA_CPP_DIR");
    println!("cargo:rerun-if-env-changed=LLAMA_CPP_LIB_DIR");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");
    if let Some(commit) = git_commit() {
        println!("cargo:rustc-env=NC_HYPNAGOGIC_COMMIT={commit}");
    }

    let lib_dir = env::var_os("LLAMA_CPP_LIB_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("LLAMA_CPP_DIR")
                .map(|root| PathBuf::from(root).join("build-cpu").join("bin"))
        });

    let Some(dir) = lib_dir else { return };
    if !dir.join("libllama.so").exists() {
        // The llama crate's build script owns the "is the backend usable"
        // diagnostic and gives a better message; staying quiet here avoids two
        // reports of one problem.
        return;
    }
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
}
