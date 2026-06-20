//! `beth-rs` — a parser for Bethesda Creation Engine (TES3 / Morrowind) data files.
//!
//! This crate currently focuses on the TES3 `.esm`/`.esp` plugin format, decoding it
//! into typed Rust structures using [`nom`]. See [`esm`] for the public API.

pub mod esm;
pub mod types;

pub use esm::{Plugin, Record, common::EsmError};
pub use types::latin1::{L1Str, L1String};
