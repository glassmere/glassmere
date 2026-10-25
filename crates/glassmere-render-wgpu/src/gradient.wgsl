// A rounded rectangle filled with a ramp of up to eight colours. The stops
// arrive in linear light, so mixing them here is linear too. The ramp's ends
// are in the rectangle's own units, where (1, 1) is its bottom right corner,
// and a radial ramp measures distance in those units: it is an ellipse in a
// rectangle that is not square. A path is triangles over its bounds, which
// are the rectangle, with the coverage of each corner in place of the
// rounded box.
struct VertexInput {
    @location(0) position: vec2<f32>,
    // Position inside the rectangle, then its size, in logical pixels.
    @location(1) local: vec4<f32>,
    // Where the ramp starts and where it ends.
    @location(2) ends: vec4<f32>,
    // Where the stops sit along the ramp. Slots past the last stop repeat it.
    @location(3) offsets_low: vec4<f32>,
    @location(4) offsets_high: vec4<f32>,
    // Corner radius, 1 for a radial ramp, then 1 where the shape is a path and
    // its coverage is in the last component.
    @location(5) shape: vec4<f32>,
    @location(6) color0: vec4<f32>,
    @location(7) color1: vec4<f32>,
    @location(8) color2: vec4<f32>,
    @location(9) color3: vec4<f32>,
    @location(10) color4: vec4<f32>,
    @location(11) color5: vec4<f32>,
    @location(12) color6: vec4<f32>,
    @location(13) color7: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec4<f32>,
    @location(1) ends: vec4<f32>,
    @location(2) offsets_low: vec4<f32>,
    @location(3) offsets_high: vec4<f32>,
    @location(4) shape: vec4<f32>,
    @location(5) color0: vec4<f32>,
    @location(6) color1: vec4<f32>,
    @location(7) color2: vec4<f32>,
    @location(8) color3: vec4<f32>,
    @location(9) color4: vec4<f32>,
    @location(10) color5: vec4<f32>,
    @location(11) color6: vec4<f32>,
    @location(12) color7: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.local = input.local;
    output.ends = input.ends;
    output.offsets_low = input.offsets_low;
    output.offsets_high = input.offsets_high;
    output.shape = input.shape;
    output.color0 = input.color0;
    output.color1 = input.color1;
    output.color2 = input.color2;
    output.color3 = input.color3;
    output.color4 = input.color4;
    output.color5 = input.color5;
    output.color6 = input.color6;
    output.color7 = input.color7;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let size = input.local.zw;
    let local = input.local.xy;
    // Same rounded-box distance as quad.wgsl.
    let radius = min(max(input.shape.x, 0.0), min(size.x, size.y) * 0.5);
    let q = abs(local - size * 0.5) - size * 0.5 + vec2<f32>(radius);
    let distance = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    var coverage = 1.0 - smoothstep(-fwidth(distance), fwidth(distance), distance);
    if input.shape.z > 0.5 {
        coverage = input.shape.w;
    }

    let unit = local / size;
    let origin = input.ends.xy;
    let axis = input.ends.zw - origin;
    var at = dot(unit - origin, axis) / max(dot(axis, axis), 1e-12);
    if input.shape.y > 0.5 {
        at = length(unit - origin) / max(length(axis), 1e-6);
    }
    var offsets = array<f32, 8>(
        input.offsets_low.x,
        input.offsets_low.y,
        input.offsets_low.z,
        input.offsets_low.w,
        input.offsets_high.x,
        input.offsets_high.y,
        input.offsets_high.z,
        input.offsets_high.w
    );
    var colors = array<vec4<f32>, 8>(
        input.color0,
        input.color1,
        input.color2,
        input.color3,
        input.color4,
        input.color5,
        input.color6,
        input.color7
    );
    // Each segment takes over where the one before it ends, so the ramp holds
    // the first colour before the first stop and the last one after it.
    var color = colors[0];
    for (var i = 1; i < 8; i = i + 1) {
        let span = max(offsets[i] - offsets[i - 1], 1e-6);
        color = mix(color, colors[i], clamp((at - offsets[i - 1]) / span, 0.0, 1.0));
    }
    return vec4<f32>(color.rgb, color.a * coverage);
}
