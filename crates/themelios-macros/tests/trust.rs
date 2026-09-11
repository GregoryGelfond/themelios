//! Structural trust checks over Cargo's resolved graph (docs/design/macros.md
//! §10; docs/specification.md §12.5, §12.3): the direct compile-time
//! dependencies are exactly the two tiers this crate reads and expands to and
//! the proc-macro2/quote pair its codegen is written with, `syn` is declined,
//! and the crate carries no build script of its own. What is in the graph is a
//! question about the resolved graph, so it is read from `cargo metadata` —
//! Cargo's own account of it — never from a manifest's text (the reading the
//! tiers beneath established).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// The direct compile-time dependencies, exactly: the two tiers this crate
/// parses through and expands to, and the token pair its codegen is written
/// with (docs/design/macros.md §10). `syn` is declined — the surface is a
/// bespoke ASP token grammar, not Rust's.
const DIRECT_DEPENDENCIES: [&str; 4] = [
    "themelios-syntax",
    "themelios-program",
    "proc-macro2",
    "quote",
];

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Cargo's account of the workspace, resolved under the committed lock.
fn metadata() -> Value {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(manifest_dir())
        .output()
        .expect("cargo metadata runs");
    assert!(
        output.status.success(),
        "cargo metadata failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata emits JSON")
}

/// This crate's own resolved package.
fn this_package(metadata: &Value) -> &Value {
    metadata["packages"]
        .as_array()
        .expect("packages is an array")
        .iter()
        .find(|package| package["name"].as_str() == Some("themelios-macros"))
        .expect("this crate is in the graph")
}

/// The names this crate depends on over normal edges — its direct compile-time
/// dependencies; dev and build edges are outside the claim (docs/design/macros.md
/// §10; docs/specification.md §12.5).
fn direct_normal_dependencies(metadata: &Value) -> BTreeSet<String> {
    this_package(metadata)["dependencies"]
        .as_array()
        .expect("dependencies is an array")
        .iter()
        .filter(|dependency| dependency["kind"].is_null())
        .map(|dependency| {
            dependency["name"]
                .as_str()
                .expect("dependency name")
                .to_owned()
        })
        .collect()
}

#[test]
fn the_direct_dependencies_are_exactly_the_two_tiers_and_the_token_pair() {
    let metadata = metadata();
    let dependencies = direct_normal_dependencies(&metadata);
    let names: BTreeSet<&str> = dependencies.iter().map(String::as_str).collect();
    assert_eq!(
        names,
        DIRECT_DEPENDENCIES
            .iter()
            .copied()
            .collect::<BTreeSet<&str>>(),
        "docs/design/macros.md §10: the direct compile-time dependencies, exactly"
    );
}

#[test]
fn syn_is_not_a_direct_dependency() {
    let metadata = metadata();
    assert!(
        !direct_normal_dependencies(&metadata).contains("syn"),
        "docs/design/macros.md §10: syn is declined — a bespoke token grammar, not Rust's"
    );
}

#[test]
fn this_crate_declares_no_build_script() {
    let metadata = metadata();
    let has_custom_build = this_package(&metadata)["targets"]
        .as_array()
        .expect("targets is an array")
        .iter()
        .any(|target| {
            target["kind"]
                .as_array()
                .expect("target kind is an array")
                .iter()
                .any(|kind| kind.as_str() == Some("custom-build"))
        });
    assert!(
        !has_custom_build,
        "docs/specification.md §12.3: no build script of this crate's own"
    );
    assert!(
        !manifest_dir().join("build.rs").exists(),
        "docs/specification.md §12.3: no build.rs"
    );
}

#[test]
fn unsafe_code_is_forbidden_at_the_crate_root() {
    let lib = fs::read_to_string(manifest_dir().join("src/lib.rs")).expect("lib.rs is readable");
    assert!(
        lib.lines()
            .any(|line| line.trim() == "#![forbid(unsafe_code)]"),
        "docs/design/macros.md §10: forbid, not merely deny, at the root"
    );
}

#[test]
fn rust_version_floor_is_declared() {
    let manifest =
        fs::read_to_string(manifest_dir().join("Cargo.toml")).expect("manifest is readable");
    assert!(
        manifest
            .lines()
            .any(|line| line.trim() == "rust-version.workspace = true"),
        "docs/specification.md §10.1: every manifest carries the floor"
    );
}

#[test]
fn the_workspace_lint_tables_are_inherited() {
    // The clippy `pedantic` floor (docs/specification.md §5.2, §10.1) reaches
    // this crate only through `[lints] workspace = true`; the workspace `cargo
    // clippy` invocation reads the inherited table, so if this line were absent the
    // floor would silently stop running and `-D warnings` alone would not catch a
    // pedantic lint. This asserts the inheritance is present.
    let manifest =
        fs::read_to_string(manifest_dir().join("Cargo.toml")).expect("manifest is readable");
    assert!(
        manifest.contains("[lints]"),
        "docs/specification.md §10.1: the [lints] table is present"
    );
    assert!(
        manifest
            .lines()
            .any(|line| line.trim() == "workspace = true"),
        "docs/specification.md §5.2, §10.1: [lints] workspace = true, so the workspace clippy::pedantic floor is inherited, not silently dropped"
    );
}
