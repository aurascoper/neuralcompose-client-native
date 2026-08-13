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

fn main() {
    println!("cargo:rerun-if-env-changed=LLAMA_CPP_DIR");
    println!("cargo:rerun-if-env-changed=LLAMA_CPP_LIB_DIR");

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
