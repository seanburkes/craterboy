struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    var pos: vec2<f32>;
    var uv: vec2<f32>;
    if index == 0u {
        pos = vec2<f32>(-1.0, -3.0);
        uv = vec2<f32>(0.0, 2.0);
    } else if index == 1u {
        pos = vec2<f32>(3.0, 1.0);
        uv = vec2<f32>(2.0, 0.0);
    } else {
        pos = vec2<f32>(-1.0, 1.0);
        uv = vec2<f32>(0.0, 0.0);
    }

    var out: VertexOut;
    out.position = vec4<f32>(pos, 0.0, 1.0);
    out.uv = uv;
    return out;
}

struct EffectParams {
    mode: u32,
    _pad0: vec3<u32>,
    texel_size: vec2<f32>,
    smoothing_strength: f32,
    outline_strength: f32,
};

@group(0) @binding(0) var frame_tex: texture_2d<f32>;
@group(0) @binding(1) var frame_sampler: sampler;
@group(0) @binding(2) var<uniform> effect: EffectParams;

fn luma(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.299, 0.587, 0.114));
}

fn sobel(uv: vec2<f32>, texel: vec2<f32>) -> vec3<f32> {
    let tl = luma(textureSample(frame_tex, frame_sampler, uv + vec2<f32>(-texel.x, -texel.y)).rgb);
    let t = luma(textureSample(frame_tex, frame_sampler, uv + vec2<f32>(0.0, -texel.y)).rgb);
    let tr = luma(textureSample(frame_tex, frame_sampler, uv + vec2<f32>(texel.x, -texel.y)).rgb);
    let l = luma(textureSample(frame_tex, frame_sampler, uv + vec2<f32>(-texel.x, 0.0)).rgb);
    let r = luma(textureSample(frame_tex, frame_sampler, uv + vec2<f32>(texel.x, 0.0)).rgb);
    let bl = luma(textureSample(frame_tex, frame_sampler, uv + vec2<f32>(-texel.x, texel.y)).rgb);
    let b = luma(textureSample(frame_tex, frame_sampler, uv + vec2<f32>(0.0, texel.y)).rgb);
    let br = luma(textureSample(frame_tex, frame_sampler, uv + vec2<f32>(texel.x, texel.y)).rgb);

    let gx = -tl - 2.0 * l - bl + tr + 2.0 * r + br;
    let gy = -tl - 2.0 * t - tr + bl + 2.0 * b + br;
    let mag = sqrt(gx * gx + gy * gy);
    return vec3<f32>(gx, gy, mag);
}

fn smooth_color(uv: vec2<f32>, base: vec4<f32>) -> vec4<f32> {
    let texel = effect.texel_size;
    let grad = sobel(uv, texel);
    let mag = grad.z;
    if mag < 0.02 {
        return base;
    }

    let norm = normalize(grad.xy);
    let offset = norm * texel * 0.75;
    let a = textureSample(frame_tex, frame_sampler, uv + offset);
    let b = textureSample(frame_tex, frame_sampler, uv - offset);
    let blended = mix(a, b, 0.5);
    return mix(base, blended, effect.smoothing_strength);
}

fn outline_color(uv: vec2<f32>, base: vec4<f32>) -> vec4<f32> {
    let texel = effect.texel_size;
    let grad = sobel(uv, texel);
    let edge = smoothstep(0.18, 0.45, grad.z) * effect.outline_strength;
    let ink = vec3<f32>(0.04, 0.04, 0.05);
    let outlined = mix(base.rgb, ink, edge);
    return vec4<f32>(outlined, base.a);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let base = textureSample(frame_tex, frame_sampler, in.uv);
    if effect.mode == 1u {
        return smooth_color(in.uv, base);
    }
    if effect.mode == 2u {
        return outline_color(in.uv, base);
    }
    if effect.mode == 3u {
        let smoothed = smooth_color(in.uv, base);
        return outline_color(in.uv, smoothed);
    }
    return base;
}
