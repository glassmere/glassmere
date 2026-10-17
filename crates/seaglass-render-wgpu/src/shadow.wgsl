// A Gaussian-blurred rounded rectangle, in one pass. The blur along x is the
// exact integral of a Gaussian over the rounded profile, written with erf; the
// one along y is four Gaussian-weighted samples of it. Evan Wallace's
// approximation, which the CPU renderer draws too.
struct VertexInput {
    @location(0) position: vec2<f32>,
    // Position relative to the blurred rectangle's centre, then its half extents.
    @location(1) outer: vec4<f32>,
    // The same for the rectangle an inset shadow darkens the inside of.
    @location(2) inner: vec4<f32>,
    // Blur standard deviation, 1 for an inset shadow, then the corner radius of
    // the blurred rectangle and of the original one.
    @location(3) params: vec4<f32>,
    @location(4) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) outer: vec4<f32>,
    @location(1) inner: vec4<f32>,
    @location(2) params: vec4<f32>,
    @location(3) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.outer = input.outer;
    output.inner = input.inner;
    output.params = input.params;
    output.color = input.color;
    return output;
}

fn gaussian(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (sqrt(2.0 * 3.141592653589793) * sigma);
}

// Abramowitz and Stegun 7.1.26, the usual one in shader code.
fn erf1(x: f32) -> f32 {
    let s = sign(x);
    let a = abs(x);
    var v = 1.0 + (0.278393 + (0.230389 + 0.078108 * a * a) * a) * a;
    v = v * v;
    return s - s / (v * v);
}

fn shadow_x(x: f32, y: f32, sigma: f32, corner: f32, half_size: vec2<f32>) -> f32 {
    let delta = min(half_size.y - corner - abs(y), 0.0);
    let curved = half_size.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    let low = 0.5 + 0.5 * erf1((x - curved) * (sqrt(0.5) / sigma));
    let high = 0.5 + 0.5 * erf1((x + curved) * (sqrt(0.5) / sigma));
    return high - low;
}

fn rounded_box_shadow(half_size: vec2<f32>, p: vec2<f32>, sigma: f32, corner: f32) -> f32 {
    let low = p.y - half_size.y;
    let high = p.y + half_size.y;
    let start = clamp(-3.0 * sigma, low, high);
    let end = clamp(3.0 * sigma, low, high);
    let interval = (end - start) / 4.0;
    var y = start + interval * 0.5;
    var value = 0.0;
    for (var i = 0; i < 4; i = i + 1) {
        value = value + shadow_x(p.x, p.y - y, sigma, corner, half_size) * gaussian(y, sigma) * interval;
        y = y + interval;
    }
    return value;
}

// The coverage quad.wgsl computes, around a centre rather than a corner.
fn rounded(p: vec2<f32>, half_size: vec2<f32>, corner: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(corner);
    let distance = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - corner;
    return 1.0 - smoothstep(-fwidth(distance), fwidth(distance), distance);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sigma = input.params.x;
    let half_size = input.outer.zw;
    let corner = clamp(input.params.z, 0.0, min(half_size.x, half_size.y));
    // A shadow with no blur is a plain rounded rectangle.
    var alpha = rounded(input.outer.xy, half_size, corner);
    if sigma > 0.0 {
        alpha = rounded_box_shadow(half_size, input.outer.xy, sigma, corner);
    }
    if input.params.y > 0.5 {
        let half_inner = input.inner.zw;
        let inner = clamp(input.params.w, 0.0, min(half_inner.x, half_inner.y));
        // Inverted, and kept inside the rectangle it darkens.
        alpha = (1.0 - alpha) * rounded(input.inner.xy, half_inner, inner);
    }
    return vec4<f32>(input.color.rgb, input.color.a * clamp(alpha, 0.0, 1.0));
}
