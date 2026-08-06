//! Command modules. One file per verb/group (plan §15).
//!
//! Registered from `main.rs`; each `Cmd` variant maps to a function that
//! takes a shared context (token flag + output format) and returns `ExitCode`.

pub mod dc;
