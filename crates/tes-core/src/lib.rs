//! `tes-core` — format-agnostic primitives shared by the Bethesda format parsers.
//!
//! This crate has no knowledge of any particular file format and no Bevy dependency. It
//! holds the pieces every parser ([`tes3_esm`](https://docs.rs/tes3-esm),
//! [`tes3_bsa`](https://docs.rs/tes3-bsa), [`tes_nif`](https://docs.rs/tes-nif)) needs:
//!
//! - [`latin1`] — the Windows-1252 string types ([`L1Str`] / [`L1String`]).
//! - [`bytes`] — small [`nom`] helpers for reading those strings and running field
//!   parsers tolerantly.
//! - [`math`] — plain numeric primitives (currently just [`math::Color`]). Deliberately
//!   Bevy/`glam`-free; downstream crates convert these into engine types.

pub mod bytes;
pub mod latin1;
pub mod math;

pub use latin1::{L1Str, L1String};
