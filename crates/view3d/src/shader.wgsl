// Parts, their edges, the background and the part-under-the-pointer pass.
//
// Every vertex carries the number of the part it belongs to. A part's colour
// comes from the palette texture at that number, so selecting, hiding or
// recolouring parts rewrites a few bytes rather than the whole model. A part
// with no opacity is hidden: its triangles are sent off the screen.

struct Globals {
    view_proj: mat4x4<f32>,
    // xyz the eye; w 1 for a parallel projection.
    eye: vec4<f32>,
    // xyz the direction the camera looks, for a parallel projection.
    forward: vec4<f32>,
    // Towards the key light and the fill light.
    key: vec4<f32>,
    fill: vec4<f32>,
    sky: vec4<f32>,
    ground: vec4<f32>,
    top: vec4<f32>,
    bottom: vec4<f32>,
    // x the height to cut away above; y 1 when cutting.
    cut: vec4<f32>,
    // x how dark an edge is against its part.
    edge: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var palette: texture_2d<f32>;

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) part: u32,
};

struct Shaded {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) colour: vec4<f32>,
    @location(3) @interpolate(flat) part: u32,
};

fn colour_of(part: u32) -> vec4<f32> {
    let width = textureDimensions(palette).x;
    return textureLoad(palette, vec2<u32>(part % width, part / width), 0);
}

@vertex
fn vs_part(v: Vertex) -> Shaded {
    var out: Shaded;
    out.colour = colour_of(v.part);
    out.world = v.position;
    out.normal = v.normal.xyz;
    out.part = v.part;
    out.clip = g.view_proj * vec4<f32>(v.position, 1.0);
    if (out.colour.a < 0.01) {
        out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
    }
    return out;
}

fn cut_away(world: vec3<f32>) -> bool {
    return g.cut.y > 0.5 && world.z > g.cut.x;
}

@fragment
fn fs_part(s: Shaded) -> @location(0) vec4<f32> {
    // The face's own normal, for a mesh that came without any. Worked out
    // before anything is discarded, while every pixel still takes part.
    let flat_normal = cross(dpdx(s.world), dpdy(s.world));
    if (cut_away(s.world)) {
        discard;
    }
    let length_n = length(s.normal);
    var n = normalize(flat_normal);
    if (length_n > 1e-4) {
        n = s.normal / length_n;
    }
    var to_eye = normalize(g.eye.xyz - s.world);
    if (g.eye.w > 0.5) {
        to_eye = -g.forward.xyz;
    }
    // Whichever way a face is wound, light the side that is seen.
    if (dot(n, to_eye) < 0.0) {
        n = -n;
    }
    let base = s.colour.rgb;
    let ambient = mix(g.ground.rgb, g.sky.rgb, n.z * 0.5 + 0.5);
    let key = max(dot(n, g.key.xyz), 0.0);
    let fill = max(dot(n, g.fill.xyz), 0.0);
    let half_way = normalize(g.key.xyz + to_eye);
    let shine = pow(max(dot(n, half_way), 0.0), 60.0) * 0.16;
    let lit = base * (ambient + vec3<f32>(key * 0.68 + fill * 0.2)) + vec3<f32>(shine);
    return vec4<f32>(lit, 1.0);
}

@fragment
fn fs_edge(s: Shaded) -> @location(0) vec4<f32> {
    if (cut_away(s.world)) {
        discard;
    }
    return vec4<f32>(s.colour.rgb * g.edge.x, 1.0);
}

@fragment
fn fs_pick(s: Shaded) -> @location(0) u32 {
    if (cut_away(s.world)) {
        discard;
    }
    return s.part + 1u;
}

struct Backdrop {
    @builtin(position) clip: vec4<f32>,
    @location(0) height: f32,
};

// One triangle over the whole view.
@vertex
fn vs_backdrop(@builtin(vertex_index) i: u32) -> Backdrop {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    var out: Backdrop;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.height = y * 0.5 + 0.5;
    return out;
}

@fragment
fn fs_backdrop(b: Backdrop) -> @location(0) vec4<f32> {
    return vec4<f32>(mix(g.bottom.rgb, g.top.rgb, clamp(b.height, 0.0, 1.0)), 1.0);
}
