//! Structural trust checks: the crate facts of docs/design/base.md §1,
//! held mechanically per docs/specification.md §12.3. These read the
//! manifests this repository owns, so plain line scans are exact.

use std::fs;
use std::path::Path;

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The lines of this crate's manifest that sit inside `[dependencies]`.
fn shipped_dependency_lines() -> Vec<String> {
    let manifest =
        fs::read_to_string(manifest_dir().join("Cargo.toml")).expect("crate manifest is readable");
    manifest
        .lines()
        .skip_while(|line| line.trim() != "[dependencies]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with('#')
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn shipped_dependency_closure_is_empty() {
    assert_eq!(
        shipped_dependency_lines(),
        Vec::<String>::new(),
        "docs/design/base.md §1: zero dependencies in the shipped closure"
    );
}

#[test]
fn no_build_script() {
    assert!(
        !manifest_dir().join("build.rs").exists(),
        "docs/specification.md §12.3: no build script outside sys crates"
    );
}

#[test]
fn unsafe_code_is_forbidden_at_the_crate_root() {
    let lib = fs::read_to_string(manifest_dir().join("src/lib.rs")).expect("lib.rs is readable");
    assert!(
        lib.contains("#![forbid(unsafe_code)]"),
        "docs/design/base.md §1: forbid, not merely deny, at the root"
    );
}

#[test]
fn rust_version_floor_is_declared() {
    let workspace = fs::read_to_string(manifest_dir().join("../../Cargo.toml"))
        .expect("workspace manifest is readable");
    assert!(
        workspace.contains("rust-version = \"1.97\""),
        "docs/specification.md §10.1: the floor, declared"
    );
    let crate_manifest =
        fs::read_to_string(manifest_dir().join("Cargo.toml")).expect("crate manifest is readable");
    assert!(
        crate_manifest.contains("rust-version.workspace = true"),
        "docs/specification.md §10.1: every manifest carries the floor"
    );
}

#[test]
fn the_only_dependency_table_is_dev() {
    // A dependency can also arrive through a target-specific table
    // ([target.'cfg(...)'.dependencies]) or [build-dependencies] —
    // routes into the shipped closure the [dependencies] scan above
    // cannot see. Any dependencies-bearing header other than the two
    // literal tables below therefore fails; still a plain line scan,
    // exact on a manifest this repository owns.
    let manifest =
        fs::read_to_string(manifest_dir().join("Cargo.toml")).expect("crate manifest is readable");
    let offending: Vec<&str> = manifest
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('[') && line.contains("dependencies"))
        .filter(|line| *line != "[dependencies]" && *line != "[dev-dependencies]")
        .collect();
    assert_eq!(
        offending,
        Vec::<&str>::new(),
        "docs/design/base.md §1: the shipped closure admits no \
         dependency table beyond the empty [dependencies]"
    );
}
