//! A consumer that depends on the test facade alone, renamed `z` (docs/design/macros.md §9,
//! §11). Its tests build every construction macro's value through the facade and compare it with
//! the value the program tier's constructors build, reached through the facade's re-exports, up
//! to and including provenance and counted repeats. It holds no dependency on
//! `themelios-program`. A test fixture, never published.
