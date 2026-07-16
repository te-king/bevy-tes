//! Terrain texture splatting for exterior cells.
//!
//! A cell's `LAND` record carries a 16×16 grid of texture indices (`VTEX`) resolving
//! through `LTEX` records to texture files — typically 5–15 distinct textures per cell,
//! with neighbouring texels blending into each other in the original engine.
//! [`TerrainSplatMaterial`] reproduces that in one draw call: the cell's distinct
//! textures are bound as a `binding_array<texture_2d<f32>>` (they vary in size and
//! compression, so a uniform-layer array texture is not an option) and a storage buffer
//! carries the 16×16 grid remapped to layer slots (a *storage* buffer because wgpu
//! forbids uniform buffers in bind groups holding a binding array); the fragment shader
//! bilinearly blends the four nearest texels' layers.
//!
//! Binding arrays need wgpu's `TEXTURE_BINDING_ARRAY` +
//! `SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING` features (available
//! on Metal, Vulkan and DX12, not WebGL); [`splat_supported`] checks, and cell spawning
//! falls back to the plain vertex-tinted material when they're absent.

use std::num::NonZero;

use bevy::app::{App, Plugin};
use bevy::asset::{Asset, Handle, embedded_asset};
use bevy::ecs::system::SystemParamItem;
use bevy::ecs::system::lifetimeless::SRes;
use bevy::image::Image;
use bevy::pbr::{Material, MaterialPlugin};
use bevy::reflect::TypePath;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{
    sampler, storage_buffer_read_only_sized, texture_2d,
};
use bevy::render::render_resource::{
    AddressMode, AsBindGroup, AsBindGroupError, BindGroupEntries, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntries, BindGroupLayoutEntry, BindingResources,
    BufferInitDescriptor, BufferUsages, FilterMode, MipmapFilterMode, PipelineCache,
    PreparedBindGroup, SamplerBindingType, SamplerDescriptor, ShaderStages, TextureSampleType,
    UnpreparedBindGroup, WgpuFeatures,
};
use bevy::render::renderer::RenderDevice;
use bevy::render::texture::{FallbackImage, GpuImage};
use bevy::shader::ShaderRef;
use tes3_esm::records::land::VTEX_GRID;

/// Maximum distinct textures per cell — the fixed size of the shader's binding array.
/// Vanilla cells stay well under it; overflowing texels fall back to layer 0.
pub const MAX_TERRAIN_LAYERS: usize = 16;

/// Registers the [`TerrainSplatMaterial`] render pipeline. Unlike `TesPlugin`, add this
/// **after** `DefaultPlugins` (it needs the render app; and adding a `MaterialPlugin`
/// from another plugin's `finish` would silently skip its own `finish`). Without it,
/// terrain spawns with the plain vertex-tinted white material.
pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "terrain.wgsl");
        app.add_plugins(MaterialPlugin::<TerrainSplatMaterial>::default());
    }
}

/// Whether the render device supports the binding-array features the splat shader needs.
pub(crate) fn splat_supported(device: &RenderDevice) -> bool {
    device.features().contains(
        WgpuFeatures::TEXTURE_BINDING_ARRAY
            | WgpuFeatures::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
    )
}

/// Per-cell splat material: the cell's distinct land textures as binding-array layers,
/// plus the 16×16 `VTEX` grid remapped to layer slots. See the [module docs](self).
#[derive(Asset, TypePath, Clone, Debug)]
pub struct TerrainSplatMaterial {
    /// The distinct textures, at most [`MAX_TERRAIN_LAYERS`]; unused array slots are
    /// bound to the fallback image.
    pub layers: Vec<Handle<Image>>,
    /// Layer slot per `VTEX` texel, row-major from the cell's south-west corner
    /// (the [`Land::decode_textures`](tes3_esm::records::land::Land::decode_textures)
    /// ordering).
    pub indices: [u32; VTEX_GRID * VTEX_GRID],
}

impl Material for TerrainSplatMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_tes/terrain.wgsl".into()
    }
}

// Binding arrays are beyond the `AsBindGroup` derive (a `Vec<Handle<Image>>` has no
// owned-binding representation), so the bind group is built by hand, modeled on Bevy's
// `texture_binding_array` example: `unprepared_bind_group` opts out via
// `CreateBindGroupDirectly`, routing preparation through `as_bind_group`.
impl AsBindGroup for TerrainSplatMaterial {
    type Data = ();

    type Param = (SRes<RenderAssets<GpuImage>>, SRes<FallbackImage>);

    fn as_bind_group(
        &self,
        layout: &BindGroupLayoutDescriptor,
        render_device: &RenderDevice,
        pipeline_cache: &PipelineCache,
        (images, fallback): &mut SystemParamItem<'_, '_, Self::Param>,
    ) -> Result<PreparedBindGroup, AsBindGroupError> {
        // Every layer must be GPU-resident before the material can bind; retry until
        // the images finish loading. Unused slots bind the fallback image.
        let mut views = [&*fallback.d2.texture_view; MAX_TERRAIN_LAYERS];
        for (slot, handle) in self.layers.iter().take(MAX_TERRAIN_LAYERS).enumerate() {
            match images.get(handle) {
                Some(image) => views[slot] = &*image.texture_view,
                None => return Err(AsBindGroupError::RetryNextUpdate),
            }
        }

        // One deterministic repeat sampler for every layer, regardless of the settings
        // the images were loaded with (land textures always tile).
        let sampler = render_device.create_sampler(&SamplerDescriptor {
            label: Some("terrain_splat_sampler"),
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::Repeat,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: MipmapFilterMode::Linear,
            ..Default::default()
        });

        let index_buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("terrain_splat_indices"),
            contents: &self
                .indices
                .iter()
                .flat_map(|i| i.to_le_bytes())
                .collect::<Vec<u8>>(),
            usage: BufferUsages::STORAGE,
        });

        // The bind group keeps the sampler and buffer alive (wgpu resources are
        // internally reference-counted).
        let bind_group = render_device.create_bind_group(
            Self::label(),
            &pipeline_cache.get_bind_group_layout(layout),
            &BindGroupEntries::sequential((&views[..], &sampler, index_buffer.as_entire_binding())),
        );

        Ok(PreparedBindGroup {
            bindings: BindingResources(vec![]),
            bind_group,
        })
    }

    fn bind_group_data(&self) -> Self::Data {}

    fn unprepared_bind_group(
        &self,
        _layout: &BindGroupLayout,
        _render_device: &RenderDevice,
        _param: &mut SystemParamItem<'_, '_, Self::Param>,
        _force_no_bindless: bool,
    ) -> Result<UnpreparedBindGroup, AsBindGroupError> {
        Err(AsBindGroupError::CreateBindGroupDirectly)
    }

    fn bind_group_layout_entries(
        _: &RenderDevice,
        _force_no_bindless: bool,
    ) -> Vec<BindGroupLayoutEntry>
    where
        Self: Sized,
    {
        BindGroupLayoutEntries::with_indices(
            ShaderStages::FRAGMENT,
            (
                // @binding(0) var layers: binding_array<texture_2d<f32>>;
                (
                    0,
                    texture_2d(TextureSampleType::Float { filterable: true })
                        .count(NonZero::<u32>::new(MAX_TERRAIN_LAYERS as u32).unwrap()),
                ),
                // @binding(1) var layer_sampler: sampler;
                (1, sampler(SamplerBindingType::Filtering)),
                // @binding(2) var<storage, read> splat_map: array<u32, 256>;
                // A storage (not uniform) buffer: wgpu forbids uniform buffers in bind
                // groups that hold a binding array.
                (
                    2,
                    storage_buffer_read_only_sized(
                        false,
                        NonZero::<u64>::new((VTEX_GRID * VTEX_GRID * 4) as u64),
                    ),
                ),
            ),
        )
        .to_vec()
    }

    fn label() -> &'static str {
        "terrain_splat_material"
    }
}
