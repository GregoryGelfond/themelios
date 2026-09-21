//! Construction macros for themelios — sugar over the program tier's
//! constructors. Each macro is a `themelios_syntax` token source that parses
//! ASP at compile time and codegens `themelios_program` constructor calls
//! (docs/design/macros.md).
#![forbid(unsafe_code)]

mod codegen;
mod diagnostics;
mod engine;
mod source;
