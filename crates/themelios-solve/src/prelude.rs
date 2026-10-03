//! The curated names a client of the solve tier imports with one `use`: the
//! driving vocabulary — `use themelios_solve::prelude::*;`.
//!
//! Flat here are the three faces a client of the tier meets. The agent and its
//! reasoning loop: [`Agent`]; the [`Scenario`] and [`Assumption`] a scoped
//! question takes; the per-question [`SolveOptions`]; the [`StatementId`] and
//! [`Observation`] an assertion hands back, with the [`RetractionClass`] each
//! discloses; the [`Interrupt`] handle; and [`Facts`], the bulk source `observe`
//! takes. The outcome a question hands back: [`Determination`] and
//! [`Conclusion`]; the [`Solved`] and [`Optimized`] handles, with the
//! [`Incumbent`] an improving trajectory yields; the [`Models`] payload, the
//! [`Model`] unit, the [`AnswerSet`] a model's atoms are, and the [`Shown`]
//! display the core derives under a program's [`ShowRule`]; the inconclusive
//! [`Partial`], its [`Stopped`] reason, and the [`Truncation`] it concluded at;
//! the inconsistent [`Unsat`] and its [`Refutation`]; the completeness refusal
//! [`NotExhausted`]; [`Consequences`] and their [`Mode`]; and the [`Fault`] with
//! its [`Locus`], what it [`Refused`], and the [`Presupposition`] a refused
//! request names. And the contract a backend implements: [`Backend`] and its
//! [`Capabilities`], each named as a [`Capability`], with
//! [`ConsequenceSupport`]; the [`SolveRequest`], [`OptimizeRequest`],
//! [`ConsequenceRequest`], and [`GroundOptions`] it is handed; the
//! [`TruthValue`] an external is assigned; the [`Door`] a program is lowered
//! through, with the [`Admitted`] parse Door A carries and the [`NotAdmitted`]
//! refusal of one; the [`GroundProgram`] it exposes, its [`GroundRule`]s, and
//! their [`Grain`]; the [`Run`] a solve streams through; the [`NativeAnswer`]
//! its consequence door reports; and the [`Cancel`] primitive it offers.
//!
//! NOT here — and this is the rule that keeps this prelude globbable beside the
//! program, syntax, and query preludes — is a spelling one of them flattens for
//! another item, or one too generic to glob into a consumer's scope: the
//! extension traits beyond `Facts` (`Function`, `Propagator`, `Extract`) are
//! reached by `themelios_solve::extend`, the statistics by
//! `themelios_solve::outcome`, and the conformance suite as
//! `themelios_solve::conformance`. The compile-lock is `tests/cross_prelude.rs`.

pub use crate::agent::{
    Agent, Assumption, Interrupt, Observation, RetractionClass, Scenario, SolveOptions, StatementId,
};
pub use crate::bridge::{Admitted, Door, Grain, GroundProgram, GroundRule, NotAdmitted};
pub use crate::contract::{
    Backend, Cancel, Capabilities, Capability, ConsequenceRequest, ConsequenceSupport, Fault,
    GroundOptions, Locus, Mode, OptimizeRequest, Presupposition, Refused, SolveRequest, TruthValue,
};
pub use crate::extend::Facts;
pub use crate::outcome::{
    AnswerSet, Conclusion, Consequences, Determination, Incumbent, Model, Models, NativeAnswer,
    NotExhausted, Optimized, Partial, Refutation, Run, ShowRule, Shown, Solved, Stopped,
    Truncation, Unsat,
};
