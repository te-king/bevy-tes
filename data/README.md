# `data/` — local game-asset fixtures

Tests and examples read Morrowind assets from here. These files are large, copyrighted
Bethesda content and are **not** redistributable, so the whole directory is gitignored
(only this README is tracked). Supply your own copies from a Morrowind installation.

Nothing here is required to build; tests that need a missing fixture skip themselves, so a
fresh checkout without any assets still passes `cargo test`.

## Expected layout

```
data/
  meshes/      # .nif / .NIF models
    BeerBarrel.NIF
    Raindrop.nif
    cursor.nif
    fire_small.nif   # a particle effect — used to test the unsupported-block path
  textures/    # .dds / .tga textures referenced by the meshes
    Tx_BeerStein.dds # base-colour texture for BeerBarrel.NIF
```

Meshes reference their textures by filename (e.g. `BeerBarrel.NIF` names
`Tx_BeerStein.dds`); the loader resolves those names against `data/textures/`.

`Morrowind.bsa` (used by `tes3-bsa` / `tes-nif` archive tests) still lives next to those
crates under `crates/tes3-bsa/tests/`, not here.
