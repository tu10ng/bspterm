fn main() {
    if let Ok(bundled) = std::env::var("BSPTERM_BUNDLE") {
        println!("cargo:rustc-env=BSPTERM_BUNDLE={}", bundled);
    }
}
