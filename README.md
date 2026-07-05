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
| [`bevy-beth`](crates/bevy-beth) | all of the above | Bevy plugin: asset loaders + NIF→mesh/material conversion |

The parser crates know nothing about Bevy; only `bevy-beth` bridges the two. Everything
below `bevy-beth` also works standalone for tooling (see the `tes3-bsa` CLI example).

## Status

- **ESM/ESP** — full record coverage of `Morrowind.esm`, `Tribunal.esm`, `Bloodmoon.esm`.
- **BSA** — full archive read of all three shipped archives.
- **NIF** — static meshes parse and render with composed scene-graph transforms,
  per-shape base-colour textures (DDS/TGA) and materials. ~85% of Morrowind's 5,798
  models; the remainder use animation/particle/skinning blocks not yet decoded.
- **Bevy** — `BethPlugin` registers `AssetLoader`s for all three formats.

## Quickstart

You need your own copy of the game data (see [Game data](#game-data)); the code builds and
tests pass without it.

```sh
# Render a NIF to a PNG (writes the screenshot path to stdout):
cargo run -p bevy-beth --example render_nif --features render -- \
    data/meshes/i/In_De_Shack_01.nif

# ...or open a live, rotating viewer:
cargo run -p bevy-beth --example render_nif --features render -- \
    data/meshes/i/In_De_Shack_01.nif --interactive

# Load an ESM through Bevy's AssetServer and summarize it:
cargo run -p bevy-beth --example load_esm -- data/Morrowind.esm

# Poke at archives/plugins from the command line:
cargo run -p tes3-bsa --example cli -- bsa list data/Morrowind.bsa --limit 20
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
