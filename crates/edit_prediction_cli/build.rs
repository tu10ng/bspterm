fn main() {
    let cargo_toml =
        std::fs::read_to_string("../bspterm/Cargo.toml").expect("Failed to read crates/bspterm/Cargo.toml");
    let version = cargo_toml
        .lines()
        .find(|line| line.starts_with("version = "))
        .expect("Version not found in crates/bspterm/Cargo.toml")
        .split('=')
        .nth(1)
        .expect("Invalid version format")
        .trim()
        .trim_matches('"');
    println!("cargo:rustc-env=BSPTERM_PKG_VERSION={}", version);
}
