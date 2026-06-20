//! Reusable data types that are independent of the ESM/plugin format itself.
//!
//! Currently this houses [`latin1`], the Windows-1252 string types ([`L1Str`] /
//! [`L1String`]) used throughout parsing. Future format-agnostic types (e.g. BSA
//! support) can live here too.

pub mod latin1;

pub use latin1::{L1Str, L1String};
