// Draws part of an image texture into a rounded rectangle. The texture holds
// premultiplied linear colour (see `image.rs`), and so does the output.
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) local: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) tint: vec4<f32>,
    // x: corner radius, y: 1 when `tint` replaces the image's colour.
    @location(5) params: vec2<f32>,
    // Texel centres the sample may not leave: min.xy, max.xy.
    @location(6) bounds: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) local: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) tint: vec4<f32>,
    @location(4) params: vec2<f32>,
    @location(5) bounds: vec4<f32>,
};

@group(0) @binding(0) var image_texture: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.uv = input.uv;
    output.local = input.local;
    output.size = input.size;
    output.tint = input.tint;
    output.params = input.params;
    output.bounds = input.bounds;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Same rounded-box distance as quad.wgsl.
    let radius = min(max(input.params.x, 0.0), min(input.size.x, input.size.y) * 0.5);
    let q = abs(input.local - input.size * 0.5) - input.size * 0.5 + vec2<f32>(radius);
    let distance = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    let coverage = 1.0 - smoothstep(-fwidth(distance), fwidth(distance), distance);
    let uv = clamp(input.uv, input.bounds.xy, input.bounds.zw);
    let texel = textureSample(image_texture, image_sampler, uv);
    let rgb = mix(texel.rgb, input.tint.rgb * texel.a, input.params.y);
    return vec4<f32>(rgb, texel.a) * (input.tint.a * coverage);
}
