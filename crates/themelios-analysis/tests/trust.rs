//! Structural trust checks over Cargo's resolved graph
//! (docs/design/analysis.md §1, §12; docs/specification.md §12.3): the shipped
//! closure is exactly the program tier and its own closure — the base and
//! syntax tiers and syntax's closure — FFI-free, with the build scripts
//! admitted by name and none of this crate's own. What is in the closure is a
//! question about the resolved graph, so it is read from `cargo metadata` —
//! Cargo's own account of it — never from a manifest's text (the reading the
//! base, syntax, and program tiers established).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// The shipped closure, exactly: the program tier and, beneath it, the base and
/// syntax tiers and syntax's own closure (docs/design/analysis.md §1, §12;
/// docs/design/program.md §1).
const CLOSURE: [&str; 9] = [
    "themelios-program",
    "themelios-base",
    "themelios-syntax",
    "rowan",
    "text-size",
    "rustc-hash",
    "hashbrown",
    "countme",
    "memoffset",
];

/// The build scripts inside the closure, admitted by name: memoffset's
/// compiler-feature probe, inherited through the syntax tier's closure beneath
/// the program tier (docs/design/analysis.md §12; docs/specification.md §12.3).
/// When it retires upstream, this list empties and the closure loses the crate.
const BUILD_SCRIPTS_ADMITTED: [&str; 1] = ["memoffset"];

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

/// One resolved package: what the closure checks read of it.
struct Package {
    name: String,
    links: bool,
    has_build_script: bool,
}

fn packages(metadata: &Value) -> BTreeMap<String, Package> {
    metadata["packages"]
        .as_array()
        .expect("packages is an array")
        .iter()
        .map(|package| {
            let id = package["id"].as_str().expect("package id").to_owned();
            let has_build_script = package["targets"]
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
            let package = Package {
                name: package["name"].as_str().expect("package name").to_owned(),
                links: !package["links"].is_null(),
                has_build_script,
            };
            (id, package)
        })
        .collect()
}

/// The ids reachable from this crate over normal dependency edges — the
/// shipped closure; dev and build edges are outside the claim
/// (docs/design/base.md §1's reading of docs/specification.md §12.5).
fn shipped_closure(metadata: &Value, packages: &BTreeMap<String, Package>) -> BTreeSet<String> {
    let nodes = metadata["resolve"]["nodes"]
        .as_array()
        .expect("resolve nodes");
    let node_of = |id: &str| {
        nodes
            .iter()
            .find(|node| node["id"].as_str() == Some(id))
            .unwrap_or_else(|| panic!("resolve node for {id}"))
    };
    let root = packages
        .iter()
        .find(|(_, package)| package.name == "themelios-analysis")
        .map(|(id, _)| id.clone())
        .expect("this crate is in the graph");
    let mut closure = BTreeSet::new();
    let mut frontier = vec![root];
    while let Some(id) = frontier.pop() {
        for dep in node_of(&id)["deps"].as_array().expect("deps is an array") {
            let normal = dep["dep_kinds"]
                .as_array()
                .expect("dep_kinds is an array")
                .iter()
                .any(|kind| kind["kind"].is_null());
            if !normal {
                continue;
            }
            let pkg = dep["pkg"].as_str().expect("dep pkg id").to_owned();
            if closure.insert(pkg.clone()) {
                frontier.push(pkg);
            }
        }
    }
    closure
}

#[test]
fn the_shipped_closure_is_exactly_the_program_tier_and_its_closure() {
    let metadata = metadata();
    let packages = packages(&metadata);
    let names: BTreeSet<&str> = shipped_closure(&metadata, &packages)
        .iter()
        .map(|id| packages[id].name.as_str())
        .collect();
    assert_eq!(
        names,
        CLOSURE.iter().copied().collect::<BTreeSet<&str>>(),
        "docs/design/analysis.md §1, §12: the shipped closure, exactly"
    );
}

#[test]
fn the_closure_is_ffi_free_with_build_scripts_admitted_by_name() {
    let metadata = metadata();
    let packages = packages(&metadata);
    let closure = shipped_closure(&metadata, &packages);
    let mut scripted = BTreeSet::new();
    for id in &closure {
        let package = &packages[id];
        assert!(
            !package.links,
            "docs/specification.md §12.3: {} links native code",
            package.name
        );
        assert!(
            !package.name.ends_with("-sys"),
            "docs/specification.md §12.3: {} is a sys crate",
            package.name
        );
        if package.has_build_script {
            scripted.insert(package.name.as_str());
        }
    }
    assert_eq!(
        scripted,
        BUILD_SCRIPTS_ADMITTED
            .iter()
            .copied()
            .collect::<BTreeSet<&str>>(),
        "docs/design/analysis.md §12: the build scripts in the closure are exactly the admitted list"
    );
}

#[test]
fn this_crate_has_no_build_script() {
    let metadata = metadata();
    let packages = packages(&metadata);
    let this = packages
        .values()
        .find(|package| package.name == "themelios-analysis")
        .expect("this crate is in the graph");
    assert!(
        !this.has_build_script,
        "docs/specification.md §12.3: no build script of this crate's own"
    );
}

#[test]
fn this_crate_has_no_build_rs() {
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
        "docs/design/analysis.md §1: forbid, not merely deny, at the root"
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
