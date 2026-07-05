//! Building spawnable scenes from parsed NIFs at load time.
//!
//! [`build`] turns a [`Nif`] into the labeled sub-assets a renderable model needs — one
//! [`Mesh`] per drawable `NiTriShape` (`Mesh{i}`), deduplicated [`StandardMaterial`]s
//! (`Material{i}`) whose textures are resolved through the [`TesVfs`] and loaded as
//! dependencies of the same `tes://` source, and a [`WorldAsset`] (`Scene`) preserving
//! the model's node hierarchy as entities with local [`Transform`]s under a Z-up→Y-up
//! root. This mirrors the shape of Bevy's glTF loader output.

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use bevy::asset::{AssetPath, Handle, LoadContext};
use bevy::camera::visibility::Visibility;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::name::Name;
use bevy::ecs::world::World;
use bevy::image::{
    Image, ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::math::Quat;
use bevy::mesh::{Mesh, Mesh3d};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::transform::components::Transform;
use bevy::world_serialization::WorldAsset;
use tes_nif::{Block, BlockRef, Nif};

use crate::TesVfs;
use crate::convert;

/// The handles [`build`] emitted, stored on the `NifAsset`.
pub(crate) struct SceneOutput {
    pub scene: Handle<WorldAsset>,
    pub meshes: Vec<Handle<Mesh>>,
    pub materials: Vec<Handle<StandardMaterial>>,
}

/// One entity to spawn into the scene world. Recorded during the graph walk (while the
/// load context is busy minting labeled assets), spawned afterwards.
struct SpawnEntry {
    /// Index into the spawned-entity list; 0 is the scene root.
    parent: usize,
    transform: Transform,
    name: String,
    /// Mesh + material for `NiTriShape` entities; `None` for plain nodes.
    surface: Option<(Handle<Mesh>, Handle<StandardMaterial>)>,
}

/// Materials are deduplicated by what actually feeds the `StandardMaterial`: the resolved
/// texture path and the material colours (bit patterns, so `f32`s can key a map).
type MaterialKey = (Option<String>, Option<[u32; 7]>);

fn material_key(texture: &Option<String>, material: Option<&tes_nif::Material>) -> MaterialKey {
    let colors = material.map(|m| {
        [
            m.diffuse[0].to_bits(),
            m.diffuse[1].to_bits(),
            m.diffuse[2].to_bits(),
            m.emissive[0].to_bits(),
            m.emissive[1].to_bits(),
            m.emissive[2].to_bits(),
            m.alpha.to_bits(),
        ]
    });
    (texture.clone(), colors)
}

pub(crate) fn build(nif: &Nif, vfs: &TesVfs, load_context: &mut LoadContext<'_>) -> SceneOutput {
    let mut builder = SceneBuilder {
        nif,
        vfs,
        entries: Vec::new(),
        meshes: Vec::new(),
        materials: Vec::new(),
        material_index: HashMap::new(),
    };

    // Pass 1: walk the graph, minting labeled mesh/material assets and recording what to
    // spawn. (Labeled-asset creation needs the load context mutably, so this happens
    // before the scene's own child context below.)
    for &root in nif.scene_roots() {
        builder.walk(root, 0, &[], load_context);
    }

    // Pass 2: spawn the recorded entities into a fresh world under a Z-up→Y-up root.
    let mut world = World::new();
    let stem = load_context
        .path()
        .path()
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Nif".to_string());
    let root = world
        .spawn((
            Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
            Visibility::default(),
            Name::new(stem),
        ))
        .id();
    let mut spawned = vec![root];
    for entry in &builder.entries {
        let mut entity = world.spawn((
            entry.transform,
            Visibility::default(),
            Name::new(entry.name.clone()),
            ChildOf(spawned[entry.parent]),
        ));
        if let Some((mesh, material)) = &entry.surface {
            entity.insert((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone())));
        }
        spawned.push(entity.id());
    }

    let scene_context = load_context.begin_labeled_asset();
    let loaded = scene_context.finish(WorldAsset::new(world));
    let scene = load_context.add_loaded_labeled_asset("Scene", loaded);

    SceneOutput {
        scene,
        meshes: builder.meshes,
        materials: builder.materials,
    }
}

struct SceneBuilder<'a> {
    nif: &'a Nif,
    vfs: &'a TesVfs,
    entries: Vec<SpawnEntry>,
    meshes: Vec<Handle<Mesh>>,
    materials: Vec<Handle<StandardMaterial>>,
    material_index: HashMap<MaterialKey, Handle<StandardMaterial>>,
}

impl SceneBuilder<'_> {
    /// Depth-first walk mirroring [`Nif::instances`]'s semantics exactly: hidden and
    /// collision subtrees are skipped, and a node's own properties take precedence over
    /// inherited ones.
    fn walk(
        &mut self,
        block: BlockRef,
        parent: usize,
        inherited: &[BlockRef],
        load_context: &mut LoadContext<'_>,
    ) {
        match self.nif.block(block) {
            Some(Block::Node(node)) => {
                if node.hidden || node.collision {
                    return;
                }
                let mut props = node.properties.clone();
                props.extend_from_slice(inherited);

                self.entries.push(SpawnEntry {
                    parent,
                    transform: convert::nif_transform(&node.transform),
                    name: format!("Node{}", block.index().unwrap_or_default()),
                    surface: None,
                });
                let slot = self.entries.len(); // this entry's index in `spawned` (root offset)
                for &child in &node.children {
                    self.walk(child, slot, &props, load_context);
                }
            }
            Some(Block::TriShape(shape)) => {
                if shape.hidden {
                    return;
                }
                let Some(Block::TriShapeData(tri)) = self.nif.block(shape.data) else {
                    return;
                };
                if tri.vertices.is_empty() || tri.triangles.is_empty() {
                    return;
                }
                let mut props = shape.properties.clone();
                props.extend_from_slice(inherited);

                let mesh = load_context.add_labeled_asset(
                    format!("Mesh{}", self.meshes.len()),
                    convert::trimesh_to_mesh(tri),
                );
                self.meshes.push(mesh.clone());
                let material = self.material_for(&props, load_context);

                self.entries.push(SpawnEntry {
                    parent,
                    transform: convert::nif_transform(&shape.transform),
                    name: format!("Shape{}", block.index().unwrap_or_default()),
                    surface: Some((mesh, material)),
                });
            }
            _ => {}
        }
    }

    /// The (deduplicated) material for a shape's resolved property list, minting the
    /// labeled asset — and the texture dependency load — on first sight.
    fn material_for(
        &mut self,
        props: &[BlockRef],
        load_context: &mut LoadContext<'_>,
    ) -> Handle<StandardMaterial> {
        let material = self.nif.material(props);
        let texture = self.nif.base_texture(props).and_then(|name| {
            let name = name.decode();
            let resolved = self.vfs.resolve_texture(&name);
            if resolved.is_none() {
                eprintln!(
                    "bevy-beth: {}: texture {name:?} not found in the VFS; using tint only",
                    load_context.path()
                );
            }
            resolved
        });

        let key = material_key(&texture, material.as_ref());
        if let Some(handle) = self.material_index.get(&key) {
            return handle.clone();
        }

        let texture_handle = texture.map(|tex| {
            // A leading `/` re-roots at the same source: `tes://textures/...`.
            let path = load_context
                .path()
                .resolve(&AssetPath::parse(&format!("/{tex}")));
            load_context
                .load_builder()
                .with_settings(|s: &mut ImageLoaderSettings| {
                    s.is_srgb = true; // base-colour maps are authored in sRGB
                    let mut sampler = ImageSamplerDescriptor::default();
                    // Morrowind UVs tile beyond [0, 1]; clamping smears edge texels.
                    sampler.set_address_mode(ImageAddressMode::Repeat);
                    s.sampler = ImageSampler::Descriptor(sampler);
                })
                .load::<Image>(path)
        });

        let handle = load_context.add_labeled_asset(
            format!("Material{}", self.materials.len()),
            convert::nif_material(texture_handle, material.as_ref()),
        );
        self.materials.push(handle.clone());
        self.material_index.insert(key, handle.clone());
        handle
    }
}
