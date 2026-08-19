//! Shared corpus support for the integration tests: one walk of the
//! vendored corpus, reading each input's dialect from its `.expect`
//! sidecar. The single home of the walk the law suites share (the
//! membership authority `corpus.rs` keeps its own richer reader).

use std::fs;
use std::path::PathBuf;

use themelios_syntax::dialect::Dialect;

/// Every `.lp` input under `tests/corpus`, in path order, with its
/// dialect: the sidecar's first line — `clingo` or `asp-core-2`, no
/// other value admitted (a typo is a loud failure, not a silent Clingo)
/// — or Clingo where there is no sidecar.
pub fn corpus() -> Vec<(String, String, Dialect)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut found = Vec::new();
    let mut pending = vec![dir.clone()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).expect("corpus reads") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "lp") {
                let text = fs::read_to_string(&path).expect("input reads");
                let sidecar = path.with_extension("expect");
                let dialect = match fs::read_to_string(&sidecar) {
                    Err(_) => Dialect::Clingo,
                    Ok(expect) => match expect.lines().next() {
                        Some("clingo") => Dialect::Clingo,
                        Some("asp-core-2") => Dialect::AspCore2,
                        other => panic!("{}: dialect line, found {other:?}", sidecar.display()),
                    },
                };
                found.push((
                    path.strip_prefix(&dir)
                        .expect("under corpus")
                        .display()
                        .to_string(),
                    text,
                    dialect,
                ));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}
