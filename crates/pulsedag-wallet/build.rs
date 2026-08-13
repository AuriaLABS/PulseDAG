use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR for build scripts"),
    );
    let workspace = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("wallet crate lives under workspace/crates");
    let evidence = workspace.join("ci-evidence/workspace");
    let lockfile = workspace.join("Cargo.lock");

    if evidence.is_dir() && lockfile.is_file() {
        fs::copy(&lockfile, evidence.join("generated-Cargo.lock"))
            .expect("copy resolved Cargo.lock into CI evidence");
    }

    println!("cargo:rerun-if-changed={}", lockfile.display());
}
