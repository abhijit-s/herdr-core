include!("fork_build.rs");

fn main() {
    let dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    if let Some(build) = fork_build(&dir) {
        println!("cargo:rustc-env=HERDR_FORK_BUILD={build}");
    }
    for file in git_watch(&dir) {
        println!("cargo:rerun-if-changed={}", file.display());
    }
    // Edits under src make the tree dirty without touching the index.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=fork_build.rs");
    println!("cargo:rerun-if-changed=../../distribution/macos-Info.plist");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let plist = include_str!("../../distribution/macos-Info.plist")
        .replace("@VERSION@", &std::env::var("CARGO_PKG_VERSION").unwrap());
    let path = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("Info.plist");
    std::fs::write(&path, plist).unwrap();
    // Separate arguments preserve paths containing spaces.
    for arg in ["-sectcreate", "__TEXT", "__info_plist"] {
        println!("cargo:rustc-link-arg-bin=herdr-core=-Wl,{arg}");
    }
    println!("cargo:rustc-link-arg-bin=herdr-core={}", path.display());
}
