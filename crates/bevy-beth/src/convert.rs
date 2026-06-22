//! Conversions from parsed Bethesda data into Bevy engine types.
//!
//! This is the home for the Bevy-coupled translation layer — NIF block graphs into
//! `Mesh` / `StandardMaterial` / `Scene`, and texture blobs (DDS/TGA) into `Image`. It is
//! intentionally empty for now: it exists so that work lands here rather than bloating the
//! [`AssetLoader`](bevy::asset::AssetLoader)s in the crate root, and so the dependency
//! direction stays one-way (the parser crates know nothing of Bevy; only this crate
//! bridges the two).
//!
//! Planned: lean on the `image` / `ddsfile` crates for texture decoding rather than
//! hand-rolling DDS.
