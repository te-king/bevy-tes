# bevy-beth

Load The Elder Scrolls III: Morrowind data files into [Bevy](https://bevyengine.org).

A Rust workspace of engine-free format parsers with a Bevy integration layer on top:
parse `.esm`/`.esp` plugins, read files straight out of `.bsa` archives (zero-copy,
mmap-backed), and turn `.nif` models into textured, per-shape Bevy meshes and materials.

## Crates

| Crate | Depends on | What it does |
|---|---|---|
| [`tes-core`](crates/tes-core) | — | Shared primitives: Windows-1252 strings (`L1String`), nom helpers |
| [`tes3-esm`](crates/tes3-esm) | tes-core | Plugin (`.esm`/`.esp`) parser — all 43 TES3 record types |
| [`tes3-bsa`](crates/tes3-bsa) | tes-core | BSA archive reader — mmap + zero-copy file slices, indexed lookup |
| [`tes-nif`](crates/tes-nif) | tes-core | NIF 4.0.0.2 model parser — scene graph, geometry, textures, materials |
| [`bevy-beth`](crates/bevy-beth) | all of the above | Bevy plugin: `tes://` asset source (loose files layered over BSAs) + loaders; NIFs load as spawnable scenes |

The parser crates know nothing about Bevy; only `bevy-beth` bridges the two. Everything
below `bevy-beth` also works standalone for tooling (see the `tes3-bsa` CLI example).
There is also [`tes-testdata`](crates/tes-testdata), an internal (unpublished)
dev-dependency that locates the gitignored `data/` directory for integration tests and
encodes the skip-when-absent convention.

## Status

- **ESM/ESP** — full record coverage of `Morrowind.esm`, `Tribunal.esm`, `Bloodmoon.esm`.
- **BSA** — full archive read of all three shipped archives.
- **NIF** — every model in the vanilla corpus (all three archives plus loose files,
  14,663 NIFs) parses. Static meshes render with composed scene-graph transforms,
  per-shape base-colour textures (DDS/TGA) and materials, and `NiAlphaProperty`
  transparency (alpha-tested cutouts for foliage, blended and additive surfaces);
  animation/particle/skinning blocks are decoded far enough to walk past, so skinned
  and animated models yield their bind-pose geometry (no animation playback yet).
- **Bevy** — `BethPlugin` registers the `tes://` asset source: a case-insensitive VFS
  layering loose data files over the BSA archives, exactly as the game resolves paths.
  NIF loads emit labeled `Mesh`/`StandardMaterial`/scene sub-assets (glTF-loader style),
  with textures resolved through the same VFS — so
  `asset_server.load("tes://meshes/i/in_de_shack_01.nif#Scene")` just works, archived or
  loose.

## Quickstart

You need your own copy of the game data (see [Game data](#game-data)); the code builds and
tests pass without it.

```sh
# Render a NIF to a PNG (writes the screenshot path to stdout). The model is named by
# its game-data path and may live loose or inside a BSA archive:
cargo run -p bevy-beth --example render_nif --features render -- \
    meshes/i/in_de_shack_01.nif

# ...or open a live, rotating viewer:
cargo run -p bevy-beth --example render_nif --features render -- \
    meshes/i/in_de_shack_01.nif --interactive

# Load an ESM through Bevy's AssetServer and summarize it:
cargo run -p bevy-beth --example load_esm -- data/Morrowind.esm

# Poke at archives/plugins from the command line:
cargo run -p tes3-bsa --example cli -- list data/Morrowind.bsa --limit 20
cargo run -p tes3-esm --example inspect -- data/Morrowind.esm
```

## Game data

Morrowind's data files are copyrighted and **never** enter this repository — the
`data/` directory is gitignored (and `.gitignore` blocks the asset extensions globally as
a second layer). Copy your installation's `Data Files` contents into `data/`; the layout
is documented in [data/README.md](data/README.md). Tests that need game data skip
themselves when it's absent, so CI and fresh checkouts stay green.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual
licensed as above, without any additional terms or conditions.

This project is not affiliated with or endorsed by Bethesda Softworks. It reads the
file formats; it ships none of the game's content.
