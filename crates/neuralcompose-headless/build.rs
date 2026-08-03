//! Emits an rpath so this binary finds llama.cpp's shared libraries.
//!
//! WHY THIS EXISTS SEPARATELY FROM THE LLAMA CRATE'S BUILD SCRIPT
//!
//! `cargo:rustc-link-arg` applies only to the targets of the package that emits
//! it. `neuralcompose-llama` is a library, so the rpath it emits is not
//! inherited by binaries that depend on it, and this binary linked fine and
//! then failed at startup with `libllama.so.0: cannot open shared object file`.
//! Link *libraries* propagate through the dependency graph; link *arguments* do
//! not.
//!
//! The alternative is making callers set `LD_LIBRARY_PATH` on every invocation,
//! which fails at load time rather than build time and is easy to forget in
//! exactly the runs that produce acceptance evidence.
//!
//! Reads the same environment variables as the llama crate, and does nothing
//! when they are unset — a stub build has no shared library to find.

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
        // Not an error: the llama crate's own build script is the one that
        // decides whether the backend is usable, and it panics with a better
        // message. Staying quiet here avoids two different diagnostics for one
        // problem.
        return;
    }
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
}
