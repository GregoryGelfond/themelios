//! A differential correctness net for the most general unifier (docs/design/program.md §11.1),
//! beyond the hand-picked property laws: the near-linear Martelli–Montanari `mgu` is checked
//! against an obviously-correct reference over many generated constructor-fragment atom pairs —
//! the estate's naive-twin discipline (as `symbol.rs`/`term.rs` hold their hand-written `Ord`
//! against a naive twin). The reference is a textbook Robinson unifier (recursive substitution
//! with a full occurs check) over a representation that erases the ground/non-ground split, so it
//! shares none of `mgu`'s union-find structure. For each pair the two must agree on
//! unifiability, `mgu`'s substitution must be a unifier, and — the real most-generality check —
//! the two unified forms must be equal up to variable renaming (α-equivalence). A second,
//! feature-gated differential (`swipl-differential`) checks the same pairs against SWI-Prolog's
//! `unify_with_occurs_check/2`, an independent, battle-tested oracle.

use std::collections::BTreeMap;

use themelios_program::program::Atom;
use themelios_program::symbol::{Name, Sign, Symbol, VarName};
use themelios_program::term::{Term, Variable};
use themelios_program::unify::mgu;

// ---- term builders (the generated fragment) ----

fn name(text: &str) -> Name {
    Name::new(text).expect("a valid identifier")
}
fn tvar(text: &str) -> Term {
    Term::Variable(Variable::Named(
        VarName::new(text).expect("a valid variable name"),
    ))
}
fn konst(text: &str) -> Term {
    Term::Function {
        name: name(text),
        arguments: Vec::new(),
    }
}
fn num(value: i32) -> Term {
    Term::from(value)
}
fn func(functor: &str, arguments: Vec<Term>) -> Term {
    Term::Function {
        name: name(functor),
        arguments,
    }
}

// ---- a deterministic, dependency-free generator (SplitMix64) ----

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

const VARIABLES: [&str; 3] = ["A", "B", "C"];
const CONSTANTS: [&str; 3] = ["a", "b", "c"];
const UNARY: [&str; 2] = ["f", "g"];
const BINARY: [&str; 2] = ["h", "k"];

/// A random constructor-fragment term to the given depth budget: at the leaves a variable, a
/// constant, or a small number; deeper, also a unary or binary function or a pair tuple. The
/// small shared alphabet makes both unifiable and clashing pairs common.
fn gen_term(rng: &mut Rng, depth: u32) -> Term {
    let arms = if depth == 0 { 3 } else { 6 };
    match rng.below(arms) {
        0 => tvar(VARIABLES[rng.below(VARIABLES.len() as u64) as usize]),
        1 => konst(CONSTANTS[rng.below(CONSTANTS.len() as u64) as usize]),
        2 => num(rng.below(4) as i32),
        3 => func(
            UNARY[rng.below(UNARY.len() as u64) as usize],
            vec![gen_term(rng, depth - 1)],
        ),
        4 => func(
            BINARY[rng.below(BINARY.len() as u64) as usize],
            vec![gen_term(rng, depth - 1), gen_term(rng, depth - 1)],
        ),
        _ => Term::Tuple(vec![gen_term(rng, depth - 1), gen_term(rng, depth - 1)]),
    }
}

/// A pair of atoms with the same predicate and arity, their arguments generated independently
/// over one shared variable pool (so the two atoms are one namespace, as `mgu` reads them).
fn gen_pair(rng: &mut Rng) -> (Atom, Atom) {
    let arity = 1 + rng.below(3) as usize;
    let left: Vec<Term> = (0..arity).map(|_| gen_term(rng, 3)).collect();
    let right: Vec<Term> = (0..arity).map(|_| gen_term(rng, 3)).collect();
    (Atom::new(name("p"), left), Atom::new(name("p"), right))
}

// ---- the reference: a representation-erasing Robinson unifier ----

/// A term for the reference unifier (§11.1): a variable, or an application over a functor whose
/// name encodes the ground/non-ground *and* sign distinctions away, so a term function `f(X)`
/// and the ground symbol `f(1)` share a functor and unify structurally — exactly the fragment
/// `mgu` unifies, but represented so the reference need not know the split.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Reference {
    Var(String),
    App(String, Vec<Reference>),
}

fn sign_tag(sign: Sign) -> &'static str {
    match sign {
        Sign::Positive => "+",
        Sign::Negative => "-",
    }
}

fn symbol_reference(symbol: &Symbol) -> Reference {
    match symbol {
        Symbol::Infimum => Reference::App("#inf".to_owned(), Vec::new()),
        Symbol::Supremum => Reference::App("#sup".to_owned(), Vec::new()),
        Symbol::Number(value) => Reference::App(format!("#{value}"), Vec::new()),
        Symbol::String(text) => Reference::App(format!("${text}"), Vec::new()),
        Symbol::Function {
            name,
            arguments,
            sign,
        } => Reference::App(
            format!("{}{}", sign_tag(*sign), name.as_str()),
            arguments.iter().map(symbol_reference).collect(),
        ),
        Symbol::Tuple(items) => Reference::App(
            "()".to_owned(),
            items.iter().map(symbol_reference).collect(),
        ),
    }
}

/// Convert a canonical constructor-fragment term to the reference representation. A term function
/// is positive (§3.3), so it shares the `+` sign tag with a positive ground symbol.
fn to_reference(term: &Term) -> Reference {
    match term {
        Term::Variable(Variable::Named(name)) => Reference::Var(name.as_str().to_owned()),
        Term::Variable(Variable::Anonymous) => {
            panic!("the differential generates only named variables")
        }
        Term::Symbolic(symbol) => symbol_reference(symbol),
        Term::Function { name, arguments } => Reference::App(
            format!("+{}", name.as_str()),
            arguments.iter().map(to_reference).collect(),
        ),
        Term::Tuple(items) => {
            Reference::App("()".to_owned(), items.iter().map(to_reference).collect())
        }
        other => panic!("the differential generates only the constructor fragment, got {other:?}"),
    }
}

/// Resolve a reference term's head through the substitution — follow variable bindings to the
/// first non-variable or unbound variable.
fn walk(term: &Reference, subst: &BTreeMap<String, Reference>) -> Reference {
    let mut current = term.clone();
    while let Reference::Var(name) = &current {
        match subst.get(name) {
            Some(bound) => current = bound.clone(),
            None => break,
        }
    }
    current
}

/// Whether a variable occurs in a term under the substitution — the full occurs check (§11.1),
/// obviously correct over the small generated terms.
fn occurs(variable: &str, term: &Reference, subst: &BTreeMap<String, Reference>) -> bool {
    match walk(term, subst) {
        Reference::Var(name) => variable == name,
        Reference::App(_, arguments) => arguments.iter().any(|arg| occurs(variable, arg, subst)),
    }
}

/// The textbook Robinson unifier (§11.1): unify a work list of term pairs into an idempotent
/// substitution, or `None` on a clash or a failed occurs check. No union-find, no triangular
/// read-out — the independent reference `mgu`'s near-linear form is checked against.
fn reference_unify(pairs: Vec<(Reference, Reference)>) -> Option<BTreeMap<String, Reference>> {
    let mut subst: BTreeMap<String, Reference> = BTreeMap::new();
    let mut work = pairs;
    work.reverse();
    while let Some((left, right)) = work.pop() {
        let left = walk(&left, &subst);
        let right = walk(&right, &subst);
        match (left, right) {
            (Reference::Var(x), Reference::Var(y)) if x == y => {}
            (Reference::Var(variable), other) | (other, Reference::Var(variable)) => {
                if occurs(&variable, &other, &subst) {
                    return None;
                }
                subst.insert(variable, other);
            }
            (Reference::App(f, fargs), Reference::App(g, gargs)) => {
                if f != g || fargs.len() != gargs.len() {
                    return None;
                }
                work.extend(fargs.into_iter().zip(gargs));
            }
        }
    }
    Some(subst)
}

/// Fully apply the substitution — resolve to the idempotent unified form.
fn apply(term: &Reference, subst: &BTreeMap<String, Reference>) -> Reference {
    match walk(term, subst) {
        Reference::Var(name) => Reference::Var(name),
        Reference::App(functor, arguments) => Reference::App(
            functor,
            arguments.iter().map(|arg| apply(arg, subst)).collect(),
        ),
    }
}

/// Rename a term's variables to `V0, V1, …` in first-occurrence order — a canonical form for the
/// α-equivalence class, so two most general unifiers' results compare equal iff they agree up to
/// consistent renaming.
fn alpha_canonical(term: &Reference) -> Reference {
    fn go(term: &Reference, mapping: &mut BTreeMap<String, String>) -> Reference {
        match term {
            Reference::Var(name) => {
                if !mapping.contains_key(name) {
                    let canonical = format!("V{}", mapping.len());
                    mapping.insert(name.clone(), canonical);
                }
                Reference::Var(mapping[name].clone())
            }
            Reference::App(functor, arguments) => Reference::App(
                functor.clone(),
                arguments.iter().map(|arg| go(arg, mapping)).collect(),
            ),
        }
    }
    go(term, &mut BTreeMap::new())
}

/// The reference image of an atom's arguments under the reference substitution, gathered under
/// one head so the whole tuple shares its α-renaming.
fn reference_image(atom: &Atom, subst: &BTreeMap<String, Reference>) -> Reference {
    Reference::App(
        "::args".to_owned(),
        atom.argument_terms()
            .map(|term| apply(&to_reference(term), subst))
            .collect(),
    )
}

/// The `mgu` image of an atom's arguments as a reference term, gathered under the same head.
fn mgu_image(atom: &Atom, subst: &themelios_program::unify::Substitution) -> Reference {
    Reference::App(
        "::args".to_owned(),
        atom.argument_terms()
            .map(|term| to_reference(&term.clone().substitute(subst)))
            .collect(),
    )
}

const CASES: usize = 5000;

#[test]
fn mgu_agrees_with_a_naive_reference_unifier() {
    let mut rng = Rng::new(0x5EED_5EED_5EED_5EED);
    let mut unified = 0u32;
    let mut clashed = 0u32;
    for case in 0..CASES {
        let (left, right) = gen_pair(&mut rng);
        let left_terms: Vec<Reference> = left.argument_terms().map(to_reference).collect();
        let right_terms: Vec<Reference> = right.argument_terms().map(to_reference).collect();
        let reference = reference_unify(left_terms.into_iter().zip(right_terms).collect());
        // A constructor-fragment atom is always a pattern, so mgu never refuses here.
        let outcome = mgu(&left, &right).expect("a constructor-fragment atom is a pattern");

        // 1. Agreement on unifiability.
        assert_eq!(
            outcome.is_some(),
            reference.is_some(),
            "case {case}: disagree on unifiability of {left:?} vs {right:?}",
        );

        if let (Some(sigma), Some(theta)) = (&outcome, &reference) {
            unified += 1;
            // 2. mgu's substitution is a unifier: the two atoms are equal once resolved.
            let left_image: Vec<Term> = left
                .argument_terms()
                .map(|term| term.clone().substitute(sigma))
                .collect();
            let right_image: Vec<Term> = right
                .argument_terms()
                .map(|term| term.clone().substitute(sigma))
                .collect();
            assert_eq!(
                left_image, right_image,
                "case {case}: mgu is not a unifier of {left:?} vs {right:?}",
            );
            // 3. Most generality: the two most general unifiers' unified forms are α-equivalent.
            assert_eq!(
                alpha_canonical(&mgu_image(&left, sigma)),
                alpha_canonical(&reference_image(&left, theta)),
                "case {case}: mgu and the reference disagree up to renaming on {left:?} vs {right:?}",
            );
        } else {
            clashed += 1;
        }
    }
    // The generator must have exercised both outcomes, or the differential proves little.
    assert!(
        unified > 0 && clashed > 0,
        "the generator exercised both unify ({unified}) and clash ({clashed})",
    );
}

// ---- the SWI-Prolog oracle (feature-gated: needs `swipl` on PATH) ----

/// Render a reference term as Prolog source, interning each variable to a fresh Prolog variable
/// (shared across the pair, so a name is one variable in both terms). Functors are quoted atoms,
/// so the encoded names (`+f`, `#5`, `()`) are legal and distinct.
#[cfg(feature = "swipl-differential")]
fn to_prolog(term: &Reference, variables: &mut BTreeMap<String, String>) -> String {
    match term {
        Reference::Var(name) => {
            if !variables.contains_key(name) {
                let fresh = format!("V{}", variables.len());
                variables.insert(name.clone(), fresh);
            }
            variables[name].clone()
        }
        Reference::App(functor, arguments) => {
            let atom = format!("'{}'", functor.replace('\\', "\\\\").replace('\'', "\\'"));
            if arguments.is_empty() {
                atom
            } else {
                let rendered: Vec<String> = arguments
                    .iter()
                    .map(|arg| to_prolog(arg, variables))
                    .collect();
                format!("{atom}({})", rendered.join(", "))
            }
        }
    }
}

#[cfg(feature = "swipl-differential")]
#[test]
fn mgu_agrees_with_swi_prolog_on_the_constructor_fragment() {
    use std::fmt::Write as _;

    const ORACLE_CASES: usize = 2000;
    let mut rng = Rng::new(0x0DDB_A11D_0DDB_A11D);
    let mut program = String::new();
    let mut expected: Vec<bool> = Vec::new();
    for _ in 0..ORACLE_CASES {
        let (left, right) = gen_pair(&mut rng);
        let outcome = mgu(&left, &right).expect("a constructor-fragment atom is a pattern");
        expected.push(outcome.is_some());
        let mut variables: BTreeMap<String, String> = BTreeMap::new();
        let left_rendered: Vec<String> = left
            .arguments
            .iter()
            .map(|term| to_prolog(&to_reference(term), &mut variables))
            .collect();
        let right_rendered: Vec<String> = right
            .arguments
            .iter()
            .map(|term| to_prolog(&to_reference(term), &mut variables))
            .collect();
        // A per-directive clause scopes its own Prolog variables, so each pair is independent.
        writeln!(
            program,
            ":- ( unify_with_occurs_check([{}], [{}]) -> writeln('U') ; writeln('N') ).",
            left_rendered.join(", "),
            right_rendered.join(", "),
        )
        .expect("writing to a String never fails");
    }
    program.push_str(":- halt.\n");

    let path = std::env::temp_dir().join(format!("themelios_mgu_swipl_{}.pl", std::process::id()));
    std::fs::write(&path, &program).expect("write the Prolog program");
    let output = std::process::Command::new("swipl")
        .arg("-q")
        .arg(&path)
        .output()
        .expect("run swipl — is SWI-Prolog on PATH?");
    let _ = std::fs::remove_file(&path);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let results: Vec<bool> = stdout
        .lines()
        .filter_map(|line| match line.trim() {
            "U" => Some(true),
            "N" => Some(false),
            _ => None,
        })
        .collect();
    assert_eq!(
        results.len(),
        expected.len(),
        "swipl produced {} results for {} pairs; stderr: {}",
        results.len(),
        expected.len(),
        String::from_utf8_lossy(&output.stderr),
    );
    for (case, (got, want)) in results.iter().zip(&expected).enumerate() {
        assert_eq!(
            got, want,
            "case {case}: swipl and mgu disagree on unifiability",
        );
    }
}
