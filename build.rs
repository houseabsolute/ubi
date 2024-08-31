use std::env;

// mostly copied from the current_platform crate.
fn main() {
    // We need this to build `ubi` properly in the integration tests.
    for (key, value) in env::vars().filter(|(k, _)| k.starts_with("CARGO_FEATURE_")) {
        println!("cargo:rustc-env={key}={value}");
    }

    // Cargo sets the host and target env vars for build scripts, but not
    // crates:
    // https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-build-scripts
    // So we just re-export them to the crate code.
    println!("cargo:rustc-env=TARGET={}", env::var("TARGET").unwrap());
    // By default Cargo only runs the build script when a file changes.  This
    // makes it re-run on target change
    println!("cargo:rerun-if-changed-env=TARGET");
}
