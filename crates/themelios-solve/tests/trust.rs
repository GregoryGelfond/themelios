//! The solve stage's trust posture (docs/design/solve.md §16): the engine-free
//! crates — `themelios-solve` and `themelios-query` — are
//! `forbid(unsafe_code)` and FFI-free by dependency closure, and unsafe lives
//! only in the named potassco trusted computing base. What is in a closure is a
//! question about the resolved graph, so it is read from `cargo metadata
//! --locked` — Cargo's own account of it — never from a manifest's text (the
//! reading the lower tiers established).
//!
//! There is no scan of the sources for the `unsafe` token: whether a crate may
//! contain unsafe is a structural fact enforced twice — `#![forbid(unsafe_code)]`
//! at each engine-free root and the workspace `unsafe_code` lint — so a grep for
//! the word would prove nothing the attributes do not, and would fire on a doc
//! comment that merely mentions it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The engine-free crates: `forbid(unsafe_code)` at the root, FFI-free by
/// closure (docs/design/solve.md §2.1, §16).
const ENGINE_FREE: [&str; 2] = ["themelios-solve", "themelios-query"];

/// The named trusted computing base: the only crates whose lint tables allow
/// unsafe (docs/design/solve.md §2.1, §11.3, §16).
const TRUSTED_BASE: [&str; 2] = ["themelios-potassco-sys", "themelios-potassco"];

/// The FFI-free closure the engine-free crates may draw on: the lower tiers and
/// syntax's own closure, and the engine-free crates themselves. No `smallvec`:
/// the `Function` trait returns `Vec<Symbol>` (docs/design/program.md §3.4), so
/// the solve tier takes nothing beyond the lower tiers (docs/design/solve.md §16).
const PURE_CLOSURE: &[&str] = &[
    "themelios-base",
    "themelios-syntax",
    "themelios-program",
    "themelios-analysis",
    "themelios-solve",
    "themelios-query",
    "rowan",
    "text-size",
    "rustc-hash",
    "hashbrown",
    "countme",
    "memoffset",
];

/// The build scripts inside the closure, admitted by name: memoffset's
/// compiler-feature probe, inherited through the syntax tier's closure
/// (docs/design/syntax.md §14; docs/specification.md §12.3). When it retires
/// upstream, this list empties and the closure loses the crate.
const BUILD_SCRIPTS_ADMITTED: [&str; 1] = ["memoffset"];

/// The line by which a crate root forbids unsafe, outright and unoverridably.
const FORBID_UNSAFE: &str = "#![forbid(unsafe_code)]";

/// The manifest line by which a lint table allows unsafe.
const ALLOW_UNSAFE: &str = "unsafe_code = \"allow\"";

/// Cargo's account of the workspace, resolved under the committed lock.
fn cargo_metadata() -> Value {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        output.status.success(),
        "cargo metadata failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata emits JSON")
}

/// One resolved package: what the trust checks read of it.
struct Package {
    name: String,
    manifest_dir: PathBuf,
    links: bool,
    has_build_script: bool,
}

/// Every package in the resolved graph, by id.
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
            let manifest_path =
                Path::new(package["manifest_path"].as_str().expect("manifest path"));
            let package = Package {
                name: package["name"].as_str().expect("package name").to_owned(),
                manifest_dir: manifest_path
                    .parent()
                    .expect("a manifest sits in a directory")
                    .to_path_buf(),
                links: !package["links"].is_null(),
                has_build_script,
            };
            (id, package)
        })
        .collect()
}

/// The workspace's own members, by id.
fn workspace_members(metadata: &Value) -> Vec<String> {
    metadata["workspace_members"]
        .as_array()
        .expect("workspace members is an array")
        .iter()
        .map(|id| id.as_str().expect("member id").to_owned())
        .collect()
}

/// The id of the named workspace crate.
fn id_of(packages: &BTreeMap<String, Package>, crate_name: &str) -> String {
    let (id, _) = packages
        .iter()
        .find(|(_, package)| package.name == crate_name)
        .unwrap_or_else(|| panic!("{crate_name} is in the graph"));
    id.clone()
}

/// The named workspace crate's resolved package.
fn package_of<'a>(packages: &'a BTreeMap<String, Package>, crate_name: &str) -> &'a Package {
    &packages[&id_of(packages, crate_name)]
}

/// The ids reachable from the named crate over normal dependency edges — its
/// shipped closure; dev and build edges are outside the claim
/// (docs/design/base.md §1's reading of docs/specification.md §12.5).
fn shipped_closure(
    metadata: &Value,
    packages: &BTreeMap<String, Package>,
    crate_name: &str,
) -> BTreeSet<String> {
    let nodes = metadata["resolve"]["nodes"]
        .as_array()
        .expect("resolve nodes");
    let node_of = |id: &str| {
        nodes
            .iter()
            .find(|node| node["id"].as_str() == Some(id))
            .unwrap_or_else(|| panic!("resolve node for {id}"))
    };
    let mut closure = BTreeSet::new();
    let mut frontier = vec![id_of(packages, crate_name)];
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

/// The names of the crates in the named crate's shipped closure.
fn dependency_closure(metadata: &Value, crate_name: &str) -> BTreeSet<String> {
    let packages = packages(metadata);
    shipped_closure(metadata, &packages, crate_name)
        .iter()
        .map(|id| packages[id].name.clone())
        .collect()
}

/// The named crate's manifest, as text: the lint tables are a manifest fact
/// `cargo metadata` does not report.
fn manifest_of(packages: &BTreeMap<String, Package>, crate_name: &str) -> String {
    let path = package_of(packages, crate_name)
        .manifest_dir
        .join("Cargo.toml");
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("{} is readable", path.display()))
}

#[test]
fn the_engine_free_crates_draw_only_on_the_ffi_free_closure() {
    let metadata = cargo_metadata();
    for crate_name in ENGINE_FREE {
        let closure = dependency_closure(&metadata, crate_name);
        for dep in &closure {
            assert!(
                PURE_CLOSURE.contains(&dep.as_str()),
                "{crate_name} pulls in {dep}, outside the argued FFI-free closure"
            );
        }
    }
}

#[test]
fn the_engine_free_closures_link_no_native_code() {
    let metadata = cargo_metadata();
    let packages = packages(&metadata);
    for crate_name in ENGINE_FREE {
        for id in &shipped_closure(&metadata, &packages, crate_name) {
            let package = &packages[id];
            assert!(
                !package.links,
                "docs/specification.md §12.3: {crate_name} reaches {}, which links native code",
                package.name
            );
            assert!(
                !package.name.ends_with("-sys"),
                "docs/specification.md §12.3: {crate_name} reaches {}, a sys crate",
                package.name
            );
        }
    }
}

#[test]
fn the_engine_free_closures_run_only_the_admitted_build_scripts() {
    let metadata = cargo_metadata();
    let packages = packages(&metadata);
    for crate_name in ENGINE_FREE {
        let scripted: BTreeSet<&str> = shipped_closure(&metadata, &packages, crate_name)
            .iter()
            .map(|id| &packages[id])
            .filter(|package| package.has_build_script)
            .map(|package| package.name.as_str())
            .collect();
        assert_eq!(
            scripted,
            BUILD_SCRIPTS_ADMITTED
                .iter()
                .copied()
                .collect::<BTreeSet<&str>>(),
            "docs/design/solve.md §16: the build scripts in {crate_name}'s closure are exactly the admitted list"
        );
    }
}

#[test]
fn the_engine_free_crates_have_no_build_script_of_their_own() {
    let metadata = cargo_metadata();
    let packages = packages(&metadata);
    for crate_name in ENGINE_FREE {
        let package = package_of(&packages, crate_name);
        assert!(
            !package.has_build_script,
            "docs/specification.md §12.3: {crate_name} has no build script of its own"
        );
        assert!(
            !package.manifest_dir.join("build.rs").exists(),
            "docs/specification.md §12.3: {crate_name} has no build.rs"
        );
    }
}

#[test]
fn the_engine_free_crate_roots_forbid_unsafe_code() {
    let metadata = cargo_metadata();
    let packages = packages(&metadata);
    for crate_name in ENGINE_FREE {
        let root = package_of(&packages, crate_name)
            .manifest_dir
            .join("src/lib.rs");
        let lib =
            fs::read_to_string(&root).unwrap_or_else(|_| panic!("{} is readable", root.display()));
        assert!(
            lib.lines().any(|line| line.trim() == FORBID_UNSAFE),
            "docs/design/solve.md §16: {crate_name} forbids, not merely denies, unsafe at the root"
        );
    }
}

#[test]
fn only_the_potassco_trusted_base_allows_unsafe_code() {
    let metadata = cargo_metadata();
    let packages = packages(&metadata);
    let allowing: BTreeSet<&str> = workspace_members(&metadata)
        .iter()
        .map(|id| &packages[id])
        .filter(|package| {
            manifest_of(&packages, &package.name)
                .lines()
                .any(|line| line.trim() == ALLOW_UNSAFE)
        })
        .map(|package| package.name.as_str())
        .collect();
    assert_eq!(
        allowing,
        TRUSTED_BASE.iter().copied().collect::<BTreeSet<&str>>(),
        "docs/design/solve.md §16: unsafe is allowed in the named trusted computing base and nowhere else"
    );
}

#[test]
fn the_engine_free_manifests_inherit_the_workspace_lint_tables() {
    // The clippy `pedantic` floor and the workspace `unsafe_code = "deny"`
    // (docs/specification.md §5.2, §10.1) reach a crate only through `[lints]
    // workspace = true`; were the line absent, the floor would silently stop
    // running for that crate.
    let metadata = cargo_metadata();
    let packages = packages(&metadata);
    for crate_name in ENGINE_FREE {
        let manifest = manifest_of(&packages, crate_name);
        assert!(
            manifest.contains("[lints]"),
            "docs/specification.md §10.1: {crate_name} carries the [lints] table"
        );
        assert!(
            manifest
                .lines()
                .any(|line| line.trim() == "workspace = true"),
            "docs/specification.md §5.2, §10.1: {crate_name} inherits the workspace lint tables"
        );
    }
}

#[test]
fn the_solve_stage_manifests_carry_the_rust_version_floor() {
    let metadata = cargo_metadata();
    let packages = packages(&metadata);
    for crate_name in ENGINE_FREE.iter().chain(TRUSTED_BASE.iter()) {
        let manifest = manifest_of(&packages, crate_name);
        assert!(
            manifest
                .lines()
                .any(|line| line.trim() == "rust-version.workspace = true"),
            "docs/specification.md §10.1: {crate_name} carries the floor"
        );
    }
}
