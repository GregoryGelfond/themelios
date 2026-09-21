//! The construction-equality acceptance (program §16, the *first-solve*
//! witness's construction half): for every macro, the value it builds is
//! **structurally equal — up to and including provenance (`Origin::Constructed`,
//! program §6) — to the value built through the spelled-out program-tier §7.1
//! constructors** it names (docs/design/macros.md §8, §11). The codegen (AST →
//! constructor calls) and a hand-written constructor chain are two spellings of
//! one construction, and this witness holds them exact.
//!
//! The witness **accretes per macro** (TDD): each macro's first test is its
//! equality witness, comparing a value built through the macro against the same
//! value built by hand — the term-shape witnesses (every arm the codegen emits)
//! among them. That comparison needs a *compiled* macro, and no construction
//! macro exists yet — `codegen_term` builds the terms, but the first `#[proc_macro]`
//! entry point (`fact!`) lands with a later increment. So this file is the named
//! home for those cases, deliberately empty of assertions until then: a real
//! witness, not a placeholder that asserts nothing.
