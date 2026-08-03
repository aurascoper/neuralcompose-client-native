//! Compiles `csrc/nc_llama_shim.c` and links it against llama.cpp.
//!
//! Uses `std::process::Command` to drive the system compiler rather than the
//! `cc` crate, because this crate takes no build dependencies — the same reason
//! `band_power.rs` computes a DFT by hand instead of adding an FFT crate.
//!
//! llama.cpp is NOT vendored and NOT built here. It is located at build time
//! through `LLAMA_CPP_DIR`, and when that is unset the crate compiles to a stub
//! that returns `Unavailable` from every entry point. That is deliberate:
//! `cargo build` must succeed on a machine with no llama.cpp checkout, or CI on
//! four platforms would need a C++ toolchain and a 2 GB build to compile a
//! crate most of them never execute.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Declare the cfg this build script may set, so `cfg(nc_llama_stub)` is a
    // known name rather than a suspected typo.
    println!("cargo::rustc-check-cfg=cfg(nc_llama_stub)");
    println!("cargo:rerun-if-changed=csrc/nc_llama_shim.c");
    println!("cargo:rerun-if-env-changed=LLAMA_CPP_DIR");
    println!("cargo:rerun-if-env-changed=LLAMA_CPP_LIB_DIR");

    let Some(root) = env::var_os("LLAMA_CPP_DIR").map(PathBuf::from) else {
        // No checkout: build the stub. `cfg(nc_llama_stub)` is what `lib.rs`
        // keys off, so the absence of a backend is a compile-time fact rather
        // than a runtime surprise.
        println!("cargo:rustc-cfg=nc_llama_stub");
        println!(
            "cargo:warning=LLAMA_CPP_DIR unset — neuralcompose-llama built as a stub \
             (every call returns Unavailable). Set it to a llama.cpp checkout to enable."
        );
        return;
    };

    let include = root.join("include");
    let ggml_include = root.join("ggml").join("include");
    let header = include.join("llama.h");
    if !header.exists() {
        panic!(
            "LLAMA_CPP_DIR is set to {} but {} does not exist. Point it at a llama.cpp \
             checkout, or unset it to build the stub.",
            root.display(),
            header.display()
        );
    }

    // Where the built .so files live. Defaults to the layout this repo's own
    // acceptance run used; overridable because llama.cpp's build directory name
    // is a local choice, not part of its API.
    let lib_dir = env::var_os("LLAMA_CPP_LIB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("build-cpu").join("bin"));
    if !lib_dir.join("libllama.so").exists() {
        panic!(
            "no libllama.so under {}. Build llama.cpp with -DBUILD_SHARED_LIBS=ON, or set \
             LLAMA_CPP_LIB_DIR.",
            lib_dir.display()
        );
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR always set by cargo"));
    let obj = out_dir.join("nc_llama_shim.o");
    let archive = out_dir.join("libnc_llama_shim.a");

    let cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());
    run(
        Command::new(&cc)
            .args([
                "-c", "-O2", "-fPIC", "-std=c11", "-Wall", "-Wextra", "-Werror",
            ])
            .arg("-I")
            .arg(&include)
            .arg("-I")
            .arg(&ggml_include)
            .arg("-o")
            .arg(&obj)
            .arg("csrc/nc_llama_shim.c"),
        "compile shim",
    );

    let ar = env::var("AR").unwrap_or_else(|_| "ar".to_string());
    let _ = std::fs::remove_file(&archive);
    run(
        Command::new(&ar).arg("rcs").arg(&archive).arg(&obj),
        "archive shim",
    );

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=nc_llama_shim");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=llama");
    println!("cargo:rustc-link-lib=dylib=ggml");
    println!("cargo:rustc-link-lib=dylib=ggml-base");
    // rpath so the produced binary finds the .so files without the caller
    // having to set LD_LIBRARY_PATH. Without this every invocation of the
    // runtime — including the ones that produce acceptance evidence — would
    // depend on an environment variable that is easy to forget and easy to get
    // wrong, and a forgotten one fails at load time rather than at build time.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=stdc++");

    emit_provenance(&root, &lib_dir);
}

/// Record which llama.cpp produced the binary.
///
/// ADR-002's `RuntimeSmokeValidated` and `DeviceValidated` rungs both require a
/// named backend version. Reading it from git at build time means the evidence
/// cannot drift from the artefact — a hand-copied commit hash in a document can.
fn emit_provenance(root: &Path, lib_dir: &Path) {
    let commit = Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=NC_LLAMA_COMMIT={commit}");
    println!("cargo:rustc-env=NC_LLAMA_LIB_DIR={}", lib_dir.display());
}

fn run(cmd: &mut Command, what: &str) {
    match cmd.status() {
        Ok(s) if s.success() => {}
        Ok(s) => panic!("{what} failed with status {s}"),
        Err(e) => panic!("{what} could not start: {e}"),
    }
}
