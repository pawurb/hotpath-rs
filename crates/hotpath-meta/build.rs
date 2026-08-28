// Exports the compiler version for the report `meta.rustc` field; the crate's
// only build-script job.
fn main() {
    println!("cargo:rerun-if-env-changed=RUSTC");
    println!("cargo:rerun-if-env-changed=RUSTUP_TOOLCHAIN");

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    // "rustc 1.89.0 (29483883e 2025-08-04)" -> "1.89.0"
    let version = std::process::Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|s| s.split_whitespace().nth(1).map(str::to_string))
        .unwrap_or_default();
    println!("cargo:rustc-env=HOTPATH_META_RUSTC_VERSION={version}");
}
