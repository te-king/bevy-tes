// Morrowind land splat: bilinearly blend the 16×16 VTEX layer grid, then feed the
// result through Bevy's PBR pipeline (lighting, shadows, fog) with the mesh's VCLR
// vertex color as a tint. Bound by TerrainSplatMaterial's manual AsBindGroup.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_vertex_output,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var layers: binding_array<texture_2d<f32>>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var layer_sampler: sampler;
// 256 layer slots, row-major from the cell's south-west corner. A storage (not
// uniform) buffer: wgpu forbids uniform buffers alongside a binding array.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<storage, read> splat_map: array<u32, 256>;

// The layer slot at texel (x, y), clamped to the grid: border texels blend toward
// themselves rather than across the cell edge (cross-cell blending would need the
// neighbouring cells' grids).
fn layer_at(x: i32, y: i32) -> u32 {
    return splat_map[u32(clamp(y, 0, 15)) * 16u + u32(clamp(x, 0, 15))];
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    // UV_0 spans the cell 0..1 with v flipped; convert to texel units from the
    // south-west corner. Each texture repeats once per texel — the repeat sampler
    // wraps, and the unwrapped coordinate keeps derivatives continuous across texel
    // seams (fract() would break mip selection there).
    let t = vec2<f32>(in.uv.x, 1.0 - in.uv.y) * 16.0;

    // Bilinear blend of the four nearest texel centers' layers.
    let p = t - 0.5;
    let corner = vec2<i32>(floor(p));
    let f = fract(p);
    var splat = vec3<f32>(0.0);
    for (var dy = 0; dy < 2; dy++) {
        for (var dx = 0; dx < 2; dx++) {
            let w = select(1.0 - f.x, f.x, dx == 1) * select(1.0 - f.y, f.y, dy == 1);
            let layer = layer_at(corner.x + dx, corner.y + dy);
            splat += w * textureSample(layers[layer], layer_sampler, t).rgb;
        }
    }

    // base_color arrives holding the VCLR vertex color (VERTEX_COLORS); tint under it.
    var pbr_input = pbr_input_from_vertex_output(in, is_front, false);
    pbr_input.material.base_color = vec4(splat, 1.0) * pbr_input.material.base_color;
    pbr_input.material.perceptual_roughness = 1.0; // matte, like the fixed-function land

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
