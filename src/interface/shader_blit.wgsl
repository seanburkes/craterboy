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

@group(0) @binding(0) var frame_tex: texture_2d<f32>;
@group(0) @binding(1) var frame_sampler: sampler;

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(frame_tex, frame_sampler, in.uv);
}
