//! Building spawnable scenes from parsed NIFs at load time.
//!
//! [`build`] turns a [`Nif`] into the labeled sub-assets a renderable model needs — one
//! [`Mesh`] per drawable `NiTriShape` (`Mesh{i}`), deduplicated [`StandardMaterial`]s
//! (`Material{i}`) whose textures are resolved through the [`TesVfs`] and loaded as
//! dependencies of the same `tes://` source, and a [`WorldAsset`] (`Scene`) preserving
//! the model's node hierarchy as entities with local [`Transform`]s under a Z-up→Y-up,
//! game-unit→meter root. This mirrors the shape of Bevy's glTF loader output.

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
use tes_core::L1String;
use tes_nif::{Nif, WalkEvent};

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
/// base and glow texture paths, the material colours (bit patterns, so `f32`s can key a
/// map) and the alpha property's flags/threshold.
type MaterialKey = (
    Option<String>,
    Option<String>,
    Option<[u32; 7]>,
    Option<(u16, u8)>,
);

fn material_key(
    texture: &Option<String>,
    glow: &Option<String>,
    material: Option<&tes_nif::Material>,
    alpha: Option<tes_nif::AlphaProperty>,
) -> MaterialKey {
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
    (
        texture.clone(),
        glow.clone(),
        colors,
        alpha.map(|a| (a.flags, a.threshold)),
    )
}

pub(crate) fn build(nif: &Nif, vfs: &TesVfs, load_context: &mut LoadContext<'_>) -> SceneOutput {
    let mut builder = SceneBuilder {
        vfs,
        entries: Vec::new(),
        meshes: Vec::new(),
        materials: Vec::new(),
        material_index: HashMap::new(),
    };

    // Pass 1: walk the graph, minting labeled mesh/material assets and recording what to
    // spawn. (Labeled-asset creation needs the load context mutably, so this happens
    // before the scene's own child context below.) `Nif::walk` owns the traversal rules
    // (hidden/collision skipping, property inheritance); this closure only tracks the
    // parent entity with a stack and records entries. Slot 0 is the scene root, so an
    // entry's slot in `spawned` below is its `entries` index + 1.
    let mut parents = vec![0usize];
    nif.walk(|event| match event {
        WalkEvent::EnterNode { block, node } => {
            builder.entries.push(SpawnEntry {
                parent: *parents.last().expect("stack starts non-empty"),
                transform: convert::nif_transform(&node.transform),
                name: format!("Node{}", block.index().unwrap_or_default()),
                surface: None,
            });
            parents.push(builder.entries.len());
        }
        WalkEvent::LeaveNode => {
            parents.pop();
        }
        WalkEvent::Shape {
            block,
            shape,
            mesh,
            base_texture,
            glow_texture,
            material,
            alpha,
        } => {
            if mesh.vertices.is_empty() || mesh.triangles.is_empty() {
                return;
            }
            let mesh = load_context.add_labeled_asset(
                format!("Mesh{}", builder.meshes.len()),
                convert::trimesh_to_mesh(mesh),
            );
            builder.meshes.push(mesh.clone());
            let material =
                builder.material_for(base_texture, glow_texture, material, alpha, load_context);

            builder.entries.push(SpawnEntry {
                parent: *parents.last().expect("stack starts non-empty"),
                transform: convert::nif_transform(&shape.transform),
                name: format!("Shape{}", block.index().unwrap_or_default()),
                surface: Some((mesh, material)),
            });
        }
    });

    // Pass 2: spawn the recorded entities into a fresh world under a root that converts
    // the whole model at once: Z-up→Y-up rotation, game-unit→meter scale. Node
    // transforms below it stay raw NIF values.
    let mut world = World::new();
    let stem = load_context
        .path()
        .path()
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Nif".to_string());
    let root = world
        .spawn((
            Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                .with_scale(bevy::math::Vec3::splat(convert::METERS_PER_UNIT)),
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
    vfs: &'a TesVfs,
    entries: Vec<SpawnEntry>,
    meshes: Vec<Handle<Mesh>>,
    materials: Vec<Handle<StandardMaterial>>,
    material_index: HashMap<MaterialKey, Handle<StandardMaterial>>,
}

impl SceneBuilder<'_> {
    /// The (deduplicated) material for a shape's resolved surface (as delivered by a
    /// [`WalkEvent::Shape`]), minting the labeled asset — and the texture dependency
    /// loads — on first sight.
    fn material_for(
        &mut self,
        base_texture: Option<&L1String>,
        glow_texture: Option<&L1String>,
        material: Option<tes_nif::Material>,
        alpha: Option<tes_nif::AlphaProperty>,
        load_context: &mut LoadContext<'_>,
    ) -> Handle<StandardMaterial> {
        let texture = self.resolve_texture(base_texture, load_context);
        let glow = self.resolve_texture(glow_texture, load_context);

        let key = material_key(&texture, &glow, material.as_ref(), alpha);
        if let Some(handle) = self.material_index.get(&key) {
            return handle.clone();
        }

        let texture_handle = texture.map(|tex| load_texture(&tex, load_context));
        let glow_handle = glow.map(|tex| load_texture(&tex, load_context));

        let handle = load_context.add_labeled_asset(
            format!("Material{}", self.materials.len()),
            convert::nif_material(texture_handle, glow_handle, material.as_ref(), alpha),
        );
        self.materials.push(handle.clone());
        self.material_index.insert(key, handle.clone());
        handle
    }

    /// Resolve a NIF texture filename to its VFS path, warning (once per occurrence) when
    /// the file is missing.
    fn resolve_texture(
        &self,
        name: Option<&L1String>,
        load_context: &LoadContext<'_>,
    ) -> Option<String> {
        name.and_then(|name| {
            let name = name.decode();
            let resolved = self.vfs.resolve_texture(&name);
            if resolved.is_none() {
                eprintln!(
                    "bevy-tes: {}: texture {name:?} not found in the VFS; using tint only",
                    load_context.path()
                );
            }
            resolved
        })
    }
}

/// Start the dependency load for a resolved texture path within the same `tes://` source.
/// Base-colour and glow maps alike are colour data: sRGB, with repeat addressing
/// (Morrowind UVs tile beyond [0, 1]; clamping smears edge texels).
fn load_texture(tex: &str, load_context: &mut LoadContext<'_>) -> Handle<Image> {
    // A leading `/` re-roots at the same source: `tes://textures/...`.
    let path = load_context
        .path()
        .resolve(&AssetPath::parse(&format!("/{tex}")));
    load_context
        .load_builder()
        .with_settings(|s: &mut ImageLoaderSettings| {
            s.is_srgb = true;
            let mut sampler = ImageSamplerDescriptor::default();
            sampler.set_address_mode(ImageAddressMode::Repeat);
            s.sampler = ImageSampler::Descriptor(sampler);
        })
        .load::<Image>(path)
}
