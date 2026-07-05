# `data/` — local game assets

Tests and examples read Morrowind assets from here. These files are large, copyrighted
Bethesda content and are **not** redistributable, so the whole directory is gitignored
(only this README is tracked). Supply your own copies from a Morrowind installation — the
layout mirrors the game's `Data Files` directory, so pointing this at (or copying) a real
`Data Files` works directly.

Nothing here is required to build; tests that need a missing fixture skip themselves, so a
fresh checkout without any assets still passes `cargo test`.

## Layout

```
data/
  meshes/            # .nif / .NIF models, in the game's per-letter subfolders
    cursor.nif
    fire_small.nif   # a particle effect — used to test the unsupported-block path
    f/Furn_De_Table_05.nif
    i/In_De_Shack_01.nif
    ...
  textures/          # .dds / .tga textures referenced by the meshes (mostly flat)
    Tx_wood_siding.tga
    ...
  Morrowind.esm  Tribunal.esm  Bloodmoon.esm
  Morrowind.bsa  Tribunal.bsa  Bloodmoon.bsa
  Sound/ Music/ Video/ Splash/ Fonts/ Icons/ ...
```

A mesh references its textures by filename (e.g. `In_De_Shack_01.nif` names
`Tx_wood_siding.tga`); the render example resolves those names by walking up the mesh's
ancestor directories and checking each `textures/` subdirectory, so a mesh nested at
`data/meshes/i/…` finds `data/textures/…`. Because the engine sometimes ships a `.tga`-named
texture as `.dds` (and vice versa), both extensions are tried.

Note: `Morrowind.bsa` here is a *copy*; the `tes3-bsa` / `tes-nif` archive tests still read
their own copy from `crates/tes3-bsa/tests/Morrowind.bsa`.
