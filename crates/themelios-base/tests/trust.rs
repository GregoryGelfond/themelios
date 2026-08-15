//! Structural trust checks: the crate facts of docs/design/base.md §1,
//! held mechanically per docs/specification.md §12.3. These read the
//! manifests this repository owns, line by line: a manifest line is
//! trimmed, and blank and comment lines are dropped, so a claim is met
//! only by a live line, never by a commented-out or quoted copy.

use std::fs;
use std::path::Path;

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The live lines of a file: trimmed, blanks and `#` comments dropped.
fn live_lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("{} is readable", path.display()))
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// A TOML `key = value` line split at its first `=`, both sides trimmed.
fn key_value(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    Some((key.trim(), value.trim()))
}

fn crate_manifest_lines() -> Vec<String> {
    live_lines(&manifest_dir().join("Cargo.toml"))
}

#[test]
fn shipped_dependency_closure_is_empty() {
    let lines = crate_manifest_lines();
    // The table must be present, so its emptiness is a claim the scan
    // reads and not a header the scan never found.
    let header = lines
        .iter()
        .position(|line| line == "[dependencies]")
        .expect("docs/design/base.md §1: the [dependencies] table is present, and empty");
    let body: Vec<&String> = lines[header + 1..]
        .iter()
        .take_while(|line| !line.starts_with('['))
        .collect();
    assert_eq!(
        body,
        Vec::<&String>::new(),
        "docs/design/base.md §1: zero dependencies in the shipped closure"
    );
}

#[test]
fn the_only_dependency_table_is_dev() {
    // A dependency can also arrive through a target-specific table
    // ([target.'cfg(...)'.dependencies]), [build-dependencies], a
    // subtable ([dependencies.name]), or a dotted or inline-table key
    // outside any header (dependencies.name = ..., dependencies = {...},
    // target.'cfg(...)'.dependencies.name = ...) — routes into the
    // shipped closure the [dependencies] scan above cannot see. Any
    // such header or key beyond the two literal tables fails.
    let offending: Vec<String> = crate_manifest_lines()
        .into_iter()
        .filter(|line| {
            if line.starts_with('[') {
                line.contains("dependencies")
                    && line != "[dependencies]"
                    && line != "[dev-dependencies]"
            } else {
                key_value(line).is_some_and(|(key, _)| {
                    key.starts_with("dependencies")
                        || key.starts_with("build-dependencies")
                        || key.starts_with("target")
                })
            }
        })
        .collect();
    assert_eq!(
        offending,
        Vec::<String>::new(),
        "docs/design/base.md §1: the shipped closure admits no \
         dependency table beyond the empty [dependencies]"
    );
}

#[test]
fn no_build_script() {
    assert!(
        !manifest_dir().join("build.rs").exists(),
        "docs/specification.md §12.3: no build script outside sys crates"
    );
    // Cargo also runs whatever `[package] build = "..."` names.
    let build_keys: Vec<String> = crate_manifest_lines()
        .into_iter()
        .filter(|line| {
            key_value(line).is_some_and(|(key, _)| key == "build" || key == "package.build")
        })
        .collect();
    assert_eq!(
        build_keys,
        Vec::<String>::new(),
        "docs/specification.md §12.3: no build script named by the manifest either"
    );
}

#[test]
fn unsafe_code_is_forbidden_at_the_crate_root() {
    let lib = fs::read_to_string(manifest_dir().join("src/lib.rs")).expect("lib.rs is readable");
    assert!(
        lib.lines()
            .any(|line| line.trim() == "#![forbid(unsafe_code)]"),
        "docs/design/base.md §1: forbid, not merely deny, at the root"
    );
}

#[test]
fn rust_version_floor_is_declared() {
    // Declared, not pinned to a value: the value lives in the workspace
    // manifest and the CI pin (docs/specification.md §10.1), and a
    // routine floor raise must not break a test whose proposition does
    // not depend on it.
    let workspace = live_lines(&manifest_dir().join("../../Cargo.toml"));
    assert!(
        workspace
            .iter()
            .any(|line| key_value(line).is_some_and(|(key, _)| key == "rust-version")),
        "docs/specification.md §10.1: the floor, declared in the workspace manifest"
    );
    assert!(
        crate_manifest_lines().iter().any(|line| {
            key_value(line)
                .is_some_and(|(key, value)| key == "rust-version.workspace" && value == "true")
        }),
        "docs/specification.md §10.1: every manifest carries the floor"
    );
}
