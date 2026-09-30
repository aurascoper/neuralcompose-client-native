use std::process::Command;
fn git(args: &[&str]) -> String {
    String::from_utf8_lossy(&Command::new("git").args(args).output().expect("git").stdout)
        .trim()
        .into()
}
fn main() {
    for p in [
        git(&["rev-parse", "--git-path", "HEAD"]),
        git(&["rev-parse", "--git-path", "index"]),
    ] {
        println!("cargo:rerun-if-changed={p}");
    }
    println!("cargo:rerun-if-env-changed=NC_VIZ_BUILD_STAMP");
    println!(
        "cargo:rustc-env=NC_VIZ_COMMIT={}",
        git(&["rev-parse", "HEAD"])
    );
    println!(
        "cargo:rustc-env=NC_VIZ_CLEAN={}",
        git(&["status", "--porcelain"]).is_empty()
    );
    println!(
        "cargo:rustc-env=NC_VIZ_PROFILE={}",
        std::env::var("PROFILE").unwrap()
    );
    let rust = Command::new("rustc").arg("--version").output().unwrap();
    println!(
        "cargo:rustc-env=NC_VIZ_RUSTC={}",
        String::from_utf8_lossy(&rust.stdout).trim()
    );
}
