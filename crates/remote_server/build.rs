#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]
use std::process::Command;

const BSPTERM_MANIFEST: &str = include_str!("../bspterm/Cargo.toml");

fn main() {
    let bspterm_cargo_toml: cargo_toml::Manifest =
        toml::from_str(BSPTERM_MANIFEST).expect("failed to parse bspterm Cargo.toml");
    println!(
        "cargo:rustc-env=BSPTERM_PKG_VERSION={}",
        bspterm_cargo_toml.package.unwrap().version.unwrap()
    );
    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").unwrap()
    );

    // Populate git sha environment variable if git is available
    println!("cargo:rerun-if-changed=../../.git/logs/HEAD");
    if let Some(output) = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
    {
        let git_sha = String::from_utf8_lossy(&output.stdout);
        let git_sha = git_sha.trim();

        println!("cargo:rustc-env=BSPTERM_COMMIT_SHA={git_sha}");
    }
    if let Some(build_identifier) = option_env!("GITHUB_RUN_NUMBER") {
        println!("cargo:rustc-env=BSPTERM_BUILD_ID={build_identifier}");
    }
}
