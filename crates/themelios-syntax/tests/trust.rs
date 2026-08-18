//! Structural trust checks over Cargo's resolved graph
//! (docs/design/syntax.md §1, §14, §16; docs/specification.md §12.3):
//! the shipped closure is exactly the enumerated list, FFI-free, with
//! one build script admitted by name and none of this crate's own.
//! What is in the closure is a question about the resolved graph, so it
//! is read from `cargo metadata` — Cargo's own account of it — and never
//! from a manifest's text; the base tier's line scans announced this
//! move for the first non-empty closure.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// The shipped closure, exactly, in the design's order
/// (docs/design/syntax.md §14).
const CLOSURE: [&str; 7] = [
    "themelios-base",
    "rowan",
    "text-size",
    "rustc-hash",
    "hashbrown",
    "countme",
    "memoffset",
];

/// The build scripts inside the closure, admitted by name: memoffset's
/// compiler-feature probe through autocfg (docs/design/syntax.md §14;
/// docs/specification.md §12.3). Its retiring condition is stated in
/// the design; when it retires, this list empties and the closure loses
/// the crate.
const BUILD_SCRIPTS_ADMITTED: [&str; 1] = ["memoffset"];

/// The pin (docs/design/syntax.md §14).
const ROWAN_VERSION: &str = "0.17.0";

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
    version: String,
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
                version: package["version"]
                    .as_str()
                    .expect("package version")
                    .to_owned(),
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
        .find(|(_, package)| package.name == "themelios-syntax")
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
fn the_shipped_closure_is_exactly_the_enumerated_list() {
    let metadata = metadata();
    let packages = packages(&metadata);
    let names: BTreeSet<&str> = shipped_closure(&metadata, &packages)
        .iter()
        .map(|id| packages[id].name.as_str())
        .collect();
    assert_eq!(
        names,
        CLOSURE.iter().copied().collect::<BTreeSet<&str>>(),
        "docs/design/syntax.md §14: the shipped closure, exactly"
    );
}

#[test]
fn rowan_is_pinned_with_its_features_off() {
    let metadata = metadata();
    let packages = packages(&metadata);
    let closure = shipped_closure(&metadata, &packages);
    let rowan = closure
        .iter()
        .find(|id| packages[*id].name == "rowan")
        .expect("rowan is in the closure");
    assert_eq!(
        packages[rowan].version, ROWAN_VERSION,
        "docs/design/syntax.md §14: the pin"
    );
    let features = metadata["resolve"]["nodes"]
        .as_array()
        .expect("resolve nodes")
        .iter()
        .find(|node| node["id"].as_str() == Some(rowan.as_str()))
        .map(|node| node["features"].as_array().expect("features").len())
        .expect("rowan's resolve node");
    assert_eq!(
        features, 0,
        "docs/design/syntax.md §14: rowan's serde1 feature stays off"
    );
}

#[test]
fn the_closure_is_ffi_free_with_one_build_script_admitted_by_name() {
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
        "docs/design/syntax.md §14: the build scripts in the closure are exactly the admitted list"
    );
}

#[test]
fn this_crate_has_no_build_script() {
    let metadata = metadata();
    let packages = packages(&metadata);
    let this = packages
        .values()
        .find(|package| package.name == "themelios-syntax")
        .expect("this crate is in the graph");
    assert!(
        !this.has_build_script,
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
        "docs/design/syntax.md §1: forbid, not merely deny, at the root"
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
