//! GPU mesh-gradient background, ported from the reference `clef` OpenGL renderer
//! (`mesh/*.kt`). A bicubic-Hermite patch grid whose control points are perturbed
//! by value noise and coloured by sampling the (processed) album art. The GL port
//! tessellated a 50-subdivision mesh on the CPU and ran a vertex + fragment shader
//! every frame; here the tessellation is done **once per artwork** into a static
//! `skia_safe::Vertices` (Skia caches its GPU buffer), the aspect/scale/pixel
//! transform is a per-frame affine `Matrix` on the canvas (so a resize never
//! rebuilds the buffer), and every per-frame pixel effect — the album-texture
//! rotate/scale, the breathing domain-warp, the dither and the vignette — runs in
//! ONE `RuntimeEffect` SkSL fragment shader driven by a handful of uniforms. The
//! per-vertex mesh colour is combined with the shader output via
//! `drawVertices(BlendMode::Modulate)`, matching the GL `col.rgb *= v_c`.

use skia_safe::{
    runtime_effect::ChildPtr, vertices::VertexMode, AlphaType, BlendMode, Canvas, Color, ColorType,
    Data, Image, ImageInfo, ISize, Matrix, Paint, Point, RuntimeEffect, SamplingOptions, Shader,
    TileMode, Vertices,
};

/// Subdivisions per Hermite patch. The GL reference uses 50; because the mesh is
/// tessellated only when the artwork changes (never per frame), a generous value
/// is cheap. 32 keeps the one-time vertex count reasonable while the colour
/// gradient stays smooth (the fragment texture carries the fine detail anyway).
const SUBDIVISIONS: usize = 32;
/// Processed-artwork texture edge (see [`process_bitmap`]). Matches the reference
/// `ImageUtils.processBitmap` 32×32 downscale.
const TEX_SIZE: i32 = 32;

/// Ported `MeshShaders.FRAGMENT_SHADER`. `drawVertices` feeds the interpolated
/// mesh UV (`v_t`) in as the shader coordinate, so rotation/scale/vignette all key
/// off it. The GL vertex-stage breathing is folded in here as a subtle UV
/// domain-warp so the vertex buffer can stay static.
const MESH_SKSL: &str = r#"
uniform shader album;
uniform float uTexSize;
uniform float uTime;
uniform float uAmp;
uniform float uAlpha;

half4 main(float2 uv) {
    float vol = uAmp * 2.0;
    float t = uTime + uAmp;

    // Centre, then a gentle breathing domain-warp (the GL vertex breathing).
    float2 c = uv - 0.5;
    c.x += sin(uv.y * 3.0 + uTime * 0.5) * 0.03;
    c.y += cos(uv.x * 2.5 + uTime * 0.6) * 0.03;

    // Scale by (1 - volume), rotate over time, re-centre.
    float s = max(0.001, 1.0 - vol);
    float ca = cos(t * 2.0);
    float sa = sin(t * 2.0);
    float2 r = float2(ca * c.x - sa * c.y, sa * c.x + ca * c.y);
    float2 tuv = r * s + 0.5;

    float4 col = float4(album.eval(tuv * uTexSize));

    // alphaVolumeFactor: fade the layer as loudness rises.
    float alphaFactor = uAlpha * clamp(1.0 - uAmp * 0.5, 0.5, 1.0);
    col.rgb *= alphaFactor;
    col.a *= alphaFactor;

    // Hash-based dither to break up banding.
    float2 hp = fract(uv * float2(123.34, 456.21));
    hp += dot(hp, hp + 45.32);
    float dither = (fract(hp.x * hp.y) - 0.5) * (1.0 / 255.0);
    col.rgb += dither;

    // Vignette.
    float vig = smoothstep(0.8, 0.3, length(uv - 0.5));
    col.rgb *= (0.6 + 0.4 * vig);

    return half4(col);
}
"#;

/// A GPU mesh-gradient built from one piece of artwork. Rebuilt only when the
/// artwork changes; drawn every frame with just a matrix + uniform update.
pub struct MeshGradient {
    effect: RuntimeEffect,
    album: Shader,
    vertices: Vertices,
}

// The mesh's Skia handles (RuntimeEffect / Shader / Vertices) are immutable after
// construction and only ever accessed under the engine lock or on the single
// render thread — never shared concurrently — so moving one across threads (e.g.
// building it off-thread, drawing it on the render thread) is sound. Same rationale
// as `AndroidGpuRenderer`'s `unsafe impl Send`.
unsafe impl Send for MeshGradient {}

impl std::fmt::Debug for MeshGradient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeshGradient").finish_non_exhaustive()
    }
}

impl MeshGradient {
    /// Build from raw ARGB_8888 pixels (`0xAARRGGBB`, row-major, `width`×`height`).
    /// `seed` drives the deterministic control-point layout so the same song keeps
    /// a stable background across binds. Returns `None` if the shader fails to
    /// compile or the artwork is empty.
    pub fn new(pixels: &[u32], width: usize, height: usize, seed: u32) -> Option<Self> {
        if width == 0 || height == 0 || pixels.len() < width * height {
            log::warn!("[mesh] bad art dims w={} h={} len={}", width, height, pixels.len());
            return None;
        }
        let effect = match RuntimeEffect::make_for_shader(MESH_SKSL, None) {
            Ok(effect) => effect,
            Err(error) => {
                log::warn!("[mesh] SkSL compile FAILED: {}", error);
                return None;
            }
        };

        let processed = process_bitmap(pixels, width, height);
        let Some(album) = make_album_shader(&processed) else {
            log::warn!("[mesh] album shader build failed");
            return None;
        };

        let preset = generate_control_points(seed);
        let control_points = control_points_from_preset(&preset, &processed);
        let Some(vertices) = tessellate(&preset, &control_points) else {
            log::warn!("[mesh] tessellation failed (w={} h={})", preset.width, preset.height);
            return None;
        };

        Some(Self {
            effect,
            album,
            vertices,
        })
    }

    /// Draw the background across the whole `width`×`height` surface.
    ///
    /// `time` is the (loudness-paced) animation clock; `amp` is the reactive
    /// amplitude (the GL `u_amp`); `alpha` is a global fade (`u_alpha`). All of the
    /// heavy work happens on the GPU — this only concatenates an affine transform,
    /// packs 4 uniforms and issues one `drawVertices`.
    pub fn draw(&self, canvas: &Canvas, width: f32, height: f32, time: f32, amp: f32, alpha: f32) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }

        // Uniforms, packed tightly in declaration order (all scalar floats, so no
        // vec alignment ambiguity): uTexSize, uTime, uAmp, uAlpha.
        let mut uniforms = Vec::with_capacity(4 * 4);
        for v in [TEX_SIZE as f32, time, amp, alpha.clamp(0.0, 1.0)] {
            uniforms.extend_from_slice(&v.to_le_bytes());
        }
        let Some(shader) = self.effect.make_shader(
            Data::new_copy(&uniforms),
            &[ChildPtr::Shader(self.album.clone())],
            None,
        ) else {
            log::warn!("[mesh] make_shader returned None (uniform size mismatch?)");
            return;
        };

        let mut paint = Paint::default();
        paint.set_anti_alias(false);
        paint.set_shader(shader);

        canvas.save();
        canvas.concat(&mesh_to_pixel_matrix(width, height));
        // Modulate = multiply: shader_output * per-vertex mesh colour, matching the
        // GL `col.rgb *= v_c`.
        canvas.draw_vertices(&self.vertices, BlendMode::Modulate, &paint);
        canvas.restore();
    }
}

/// Affine map from mesh space (surface points in ~[-1, 1], pre-`×1.4`) to surface
/// pixels, replicating the GL vertex stage: `p *= 1.4`, aspect correction, then the
/// clip→viewport transform (with Y flipped for Skia's Y-down canvas). Because every
/// step is a per-axis scale/translate, the whole thing is one `Matrix`, so the
/// vertex buffer stays in mesh space and never rebuilds on resize.
fn mesh_to_pixel_matrix(width: f32, height: f32) -> Matrix {
    let aspect = width / height;
    let (aspect_x, aspect_y) = if aspect > 1.0 {
        (1.0, aspect)
    } else {
        (1.0 / aspect, 1.0)
    };
    let sx = 1.4 * aspect_x * width * 0.5;
    let sy = -1.4 * aspect_y * height * 0.5; // negative: clip +Y (up) → screen −Y
    let tx = width * 0.5;
    let ty = height * 0.5;
    Matrix::new_all(sx, 0.0, tx, 0.0, sy, ty, 0.0, 0.0, 1.0)
}

fn make_album_shader(processed_rgba: &[u8]) -> Option<Shader> {
    let info = ImageInfo::new(
        ISize::new(TEX_SIZE, TEX_SIZE),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    let data = Data::new_copy(processed_rgba);
    let image = Image::from_raster_data(&info, data, (TEX_SIZE * 4) as usize)?;
    // GL_MIRRORED_REPEAT + GL_LINEAR. The shader evaluates it at `uv * TEX_SIZE`,
    // so no local matrix is needed.
    image.to_shader(
        (TileMode::Mirror, TileMode::Mirror),
        SamplingOptions::default(),
        None,
    )
}

// ---------------------------------------------------------------------------
// Control-point generation (port of `ControlPointGenerator`).
// ---------------------------------------------------------------------------

struct RawControlPoint {
    cx: usize,
    cy: usize,
    x: f32,
    y: f32,
    ur: f32,
    vr: f32,
    up: f32,
    vp: f32,
}

struct Preset {
    width: usize,
    height: usize,
    points: Vec<RawControlPoint>,
}

struct ControlPoint {
    color: [f32; 3],
    location: [f32; 2],
    u_tangent: [f32; 2],
    v_tangent: [f32; 2],
}

/// A tiny deterministic xorshift RNG so control-point generation needs no `rand`
/// crate. Seeded per artwork so a song's background is stable.
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Rng(seed | 1)
    }
    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Uniform in [0, 1).
    fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }
    fn next_int(&mut self, n: u32) -> u32 {
        self.next_u32() % n
    }
}

fn generate_control_points(seed: u32) -> Preset {
    let mut rng = Rng::new(seed);
    let w = (4 + rng.next_int(3)) as usize;
    let h = (4 + rng.next_int(3)) as usize;

    let noise_offset_x = rng.next_f32() * 100.0;
    let noise_offset_y = rng.next_f32() * 100.0;

    let mut points = Vec::with_capacity(w * h);
    for j in 0..h {
        for i in 0..w {
            let u = i as f32 / (w - 1) as f32;
            let v = j as f32 / (h - 1) as f32;
            let bx = u * 2.0 - 1.0;
            let by = v * 2.0 - 1.0;
            let is_border = i == 0 || i == w - 1 || j == 0 || j == h - 1;

            let scale = 2.0;
            let nx = noise(u * scale + noise_offset_x, v * scale + noise_offset_y);
            let ny = noise(
                u * scale + noise_offset_x + 50.0,
                v * scale + noise_offset_y + 50.0,
            );
            let strength = 0.5;
            let mut x = bx + nx * strength;
            let mut y = by + ny * strength;
            if i == 0 {
                x = -1.0;
            }
            if i == w - 1 {
                x = 1.0;
            }
            if j == 0 {
                y = -1.0;
            }
            if j == h - 1 {
                y = 1.0;
            }

            let angle_noise = noise(u * 1.5 - noise_offset_x, v * 1.5 - noise_offset_y);
            let angle = angle_noise * std::f32::consts::PI;
            let rot_degrees = angle.to_degrees();
            let ur = if is_border { 0.0 } else { rot_degrees };
            let vr = if is_border { 0.0 } else { rot_degrees + 90.0 };
            let up = if is_border { 1.0 } else { rng.range(0.5, 1.5) };
            let vp = if is_border { 1.0 } else { rng.range(0.5, 1.5) };

            points.push(RawControlPoint {
                cx: i,
                cy: j,
                x,
                y,
                ur,
                vr,
                up,
                vp,
            });
        }
    }

    Preset {
        width: w,
        height: h,
        points,
    }
}

/// 2D value noise in roughly [-1, 1] (port of `ControlPointGenerator.noise`).
fn noise(x: f32, y: f32) -> f32 {
    let i = x.floor();
    let j = y.floor();
    let fx = x - i;
    let fy = y - j;
    let n00 = pseudo_rand(i, j);
    let n10 = pseudo_rand(i + 1.0, j);
    let n01 = pseudo_rand(i, j + 1.0);
    let n11 = pseudo_rand(i + 1.0, j + 1.0);
    let u = fx * fx * (3.0 - 2.0 * fx);
    let v = fy * fy * (3.0 - 2.0 * fy);
    let nx0 = mix(n00, n10, u);
    let nx1 = mix(n01, n11, u);
    mix(nx0, nx1, v)
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

fn pseudo_rand(x: f32, y: f32) -> f32 {
    let seed = x * 12.9898 + y * 78.233;
    fract(seed.sin() * 43758.547) * 2.0 - 1.0
}

fn fract(f: f32) -> f32 {
    f - f.floor()
}

fn control_points_from_preset(preset: &Preset, processed_rgba: &[u8]) -> Vec<Vec<ControlPoint>> {
    let mut grid: Vec<Vec<ControlPoint>> = (0..preset.height)
        .map(|_| {
            (0..preset.width)
                .map(|_| ControlPoint {
                    color: [1.0; 3],
                    location: [0.0; 2],
                    u_tangent: [0.0; 2],
                    v_tangent: [0.0; 2],
                })
                .collect()
        })
        .collect();

    for raw in &preset.points {
        let i = raw.cy;
        let j = raw.cx;
        if i >= preset.height || j >= preset.width {
            continue;
        }
        let pt = &mut grid[i][j];
        pt.location = [raw.x, raw.y];

        let bx = (((raw.x + 1.0) * 0.5 * (TEX_SIZE - 1) as f32) as i32).clamp(0, TEX_SIZE - 1);
        let by = (((raw.y + 1.0) * 0.5 * (TEX_SIZE - 1) as f32) as i32).clamp(0, TEX_SIZE - 1);
        let idx = ((by * TEX_SIZE + bx) * 4) as usize;
        pt.color = [
            processed_rgba[idx] as f32 / 255.0,
            processed_rgba[idx + 1] as f32 / 255.0,
            processed_rgba[idx + 2] as f32 / 255.0,
        ];

        let u_rot = raw.ur.to_radians();
        let v_rot = raw.vr.to_radians();
        pt.u_tangent = [u_rot.cos() * raw.up, u_rot.sin() * raw.up];
        pt.v_tangent = [-v_rot.sin() * raw.vp, v_rot.cos() * raw.vp];
    }

    grid
}

// ---------------------------------------------------------------------------
// Hermite math + tessellation (port of `HermiteMath` + `BHPMesh`).
// ---------------------------------------------------------------------------

type Mat4 = [f32; 16];

fn hermite(t: f32, p0: f32, p1: f32, m0: f32, m1: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    let h1 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h2 = -2.0 * t3 + 3.0 * t2;
    let h3 = t3 - 2.0 * t2 + t;
    let h4 = t3 - t2;
    h1 * p0 + h2 * p1 + h3 * m0 + h4 * m1
}

fn mesh_coefficients(
    p00: &ControlPoint,
    p01: &ControlPoint,
    p10: &ControlPoint,
    p11: &ControlPoint,
    axis: usize,
) -> Mat4 {
    let l = |p: &ControlPoint| p.location[axis];
    let u = |p: &ControlPoint| p.u_tangent[axis];
    let v = |p: &ControlPoint| p.v_tangent[axis];
    [
        l(p00),
        l(p01),
        v(p00),
        v(p01),
        l(p10),
        l(p11),
        v(p10),
        v(p11),
        u(p00),
        u(p01),
        0.0,
        0.0,
        u(p10),
        u(p11),
        0.0,
        0.0,
    ]
}

fn color_coefficients(
    p00: &ControlPoint,
    p01: &ControlPoint,
    p10: &ControlPoint,
    p11: &ControlPoint,
    axis: usize,
) -> Mat4 {
    let c = |p: &ControlPoint| p.color[axis];
    let c00 = c(p00);
    let c01 = c(p01);
    let c10 = c(p10);
    let c11 = c(p11);
    let tan_u00 = c01 - c00;
    let tan_u10 = c11 - c10;
    let tan_v00 = c10 - c00;
    let tan_v01 = c11 - c01;
    let mut m = [0.0f32; 16];
    m[0] = c00;
    m[1] = c01;
    m[2] = tan_v00;
    m[3] = tan_v01;
    m[4] = c10;
    m[5] = c11;
    m[6] = tan_v00;
    m[7] = tan_v01;
    m[8] = tan_u00;
    m[9] = tan_v01;
    m[12] = tan_u10;
    m[13] = tan_u10;
    m
}

fn surface_point(u: f32, v: f32, x: &Mat4, y: &Mat4) -> (f32, f32) {
    let eval = |m: &Mat4| {
        let p0 = hermite(u, m[0], m[1], m[8], m[9]);
        let p1 = hermite(u, m[4], m[5], m[12], m[13]);
        let m0 = hermite(u, m[2], m[3], 0.0, 0.0);
        let m1 = hermite(u, m[6], m[7], 0.0, 0.0);
        hermite(v, p0, p1, m0, m1)
    };
    (eval(x), eval(y))
}

fn color_point(u: f32, v: f32, r: &Mat4, g: &Mat4, b: &Mat4) -> [f32; 3] {
    let eval = |m: &Mat4| {
        let p0 = hermite(u, m[0], m[1], m[8], m[9]);
        let p1 = hermite(u, m[4], m[5], m[12], m[13]);
        let m0 = hermite(u, m[2], m[3], 0.0, 0.0);
        let m1 = hermite(u, m[6], m[7], 0.0, 0.0);
        hermite(v, p0, p1, m0, m1)
    };
    [eval(r), eval(g), eval(b)]
}

/// Tessellate the patch grid into a static `Vertices` in mesh space (positions in
/// ~[-1, 1], UV in [0, 1], per-vertex colour from the artwork). Ported from
/// `BHPMesh.update` + `generateIndices`.
fn tessellate(preset: &Preset, control_points: &[Vec<ControlPoint>]) -> Option<Vertices> {
    let patch_rows = preset.height.checked_sub(1)?;
    let patch_cols = preset.width.checked_sub(1)?;
    if patch_rows == 0 || patch_cols == 0 {
        return None;
    }

    // Per-patch coefficient matrices.
    let mut c_x = vec![vec![[0.0f32; 16]; patch_cols]; patch_rows];
    let mut c_y = c_x.clone();
    let mut c_r = c_x.clone();
    let mut c_g = c_x.clone();
    let mut c_b = c_x.clone();
    for j in 0..patch_rows {
        for i in 0..patch_cols {
            let p00 = &control_points[j][i];
            let p01 = &control_points[j][i + 1];
            let p10 = &control_points[j + 1][i];
            let p11 = &control_points[j + 1][i + 1];
            c_x[j][i] = mesh_coefficients(p00, p01, p10, p11, 0);
            c_y[j][i] = mesh_coefficients(p00, p01, p10, p11, 1);
            c_r[j][i] = color_coefficients(p00, p01, p10, p11, 0);
            c_g[j][i] = color_coefficients(p00, p01, p10, p11, 1);
            c_b[j][i] = color_coefficients(p00, p01, p10, p11, 2);
        }
    }

    let vertex_cols = patch_cols * SUBDIVISIONS + 1;
    let vertex_rows = patch_rows * SUBDIVISIONS + 1;
    let num_vertices = vertex_cols * vertex_rows;

    let mut positions = Vec::with_capacity(num_vertices);
    let mut texs = Vec::with_capacity(num_vertices);
    let mut colors = Vec::with_capacity(num_vertices);

    for i in 0..vertex_rows {
        let v_global = i as f32 / (vertex_rows - 1) as f32;
        for j in 0..vertex_cols {
            let u_global = j as f32 / (vertex_cols - 1) as f32;

            let pi = (j / SUBDIVISIONS).min(patch_cols - 1);
            let pj = (i / SUBDIVISIONS).min(patch_rows - 1);
            let u_local = u_global * patch_cols as f32 - pi as f32;
            let v_local = v_global * patch_rows as f32 - pj as f32;

            let (px, py) = surface_point(u_local, v_local, &c_x[pj][pi], &c_y[pj][pi]);
            positions.push(Point::new(px, py));

            let color = color_point(u_local, v_local, &c_r[pj][pi], &c_g[pj][pi], &c_b[pj][pi]);
            colors.push(Color::from_argb(
                255,
                (color[0].clamp(0.0, 1.0) * 255.0) as u8,
                (color[1].clamp(0.0, 1.0) * 255.0) as u8,
                (color[2].clamp(0.0, 1.0) * 255.0) as u8,
            ));

            // Flip V for texture coords, matching the GL `1.0 - vGlobal`.
            texs.push(Point::new(u_global, 1.0 - v_global));
        }
    }

    let mut indices = Vec::with_capacity(patch_cols * patch_rows * SUBDIVISIONS * SUBDIVISIONS * 6);
    for i in 0..vertex_rows - 1 {
        for j in 0..vertex_cols - 1 {
            let top_left = (i * vertex_cols + j) as u16;
            let top_right = (i * vertex_cols + j + 1) as u16;
            let bottom_left = ((i + 1) * vertex_cols + j) as u16;
            let bottom_right = ((i + 1) * vertex_cols + j + 1) as u16;
            indices.push(top_left);
            indices.push(bottom_left);
            indices.push(top_right);
            indices.push(top_right);
            indices.push(bottom_left);
            indices.push(bottom_right);
        }
    }

    // u16 indices cap the vertex count at 65535; SUBDIVISIONS is chosen so a
    // realistic 6×6 control grid (5 patches → 161 verts/side, ~25k) stays under.
    if num_vertices > u16::MAX as usize {
        return None;
    }

    Some(Vertices::new_copy(
        VertexMode::Triangles,
        &positions,
        &texs,
        &colors,
        Some(&indices),
    ))
}

// ---------------------------------------------------------------------------
// Artwork processing (port of `ImageUtils.processBitmap`).
// ---------------------------------------------------------------------------

/// Downscale to 32×32, boost saturation / compress lightness in HSL, then box-blur
/// (radius 2). Input is ARGB_8888 (`0xAARRGGBB`); output is 32×32 RGBA8888 bytes.
fn process_bitmap(pixels: &[u32], width: usize, height: usize) -> Vec<u8> {
    let n = (TEX_SIZE * TEX_SIZE) as usize;
    // Nearest-neighbour downscale to 32×32, adjusting HSL as we go.
    let mut small = vec![[0u8; 3]; n];
    for ty in 0..TEX_SIZE as usize {
        for tx in 0..TEX_SIZE as usize {
            let sx = (tx * width / TEX_SIZE as usize).min(width - 1);
            let sy = (ty * height / TEX_SIZE as usize).min(height - 1);
            let argb = pixels[sy * width + sx];
            let r = ((argb >> 16) & 0xff) as f32 / 255.0;
            let g = ((argb >> 8) & 0xff) as f32 / 255.0;
            let b = (argb & 0xff) as f32 / 255.0;

            let (mut h, mut s, mut l) = rgb_to_hsl(r, g, b);
            if s > 0.1 {
                s = s.clamp(0.5, 1.0);
            }
            let (min_l, max_l) = (0.15f32, 0.70f32);
            l = min_l + l * (max_l - min_l);
            let (nr, ng, nb) = hsl_to_rgb(h.rem_euclid(360.0) / 360.0, s, l);
            small[ty * TEX_SIZE as usize + tx] =
                [(nr * 255.0) as u8, (ng * 255.0) as u8, (nb * 255.0) as u8];
            let _ = &mut h;
        }
    }

    // Box blur radius 2, writing RGBA (opaque).
    let radius = 2i32;
    let size = TEX_SIZE as usize;
    let mut out = vec![0u8; n * 4];
    for y in 0..TEX_SIZE {
        for x in 0..TEX_SIZE {
            let (mut r, mut g, mut b, mut count) = (0u32, 0u32, 0u32, 0u32);
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx >= 0 && nx < TEX_SIZE && ny >= 0 && ny < TEX_SIZE {
                        let px = small[(ny * TEX_SIZE + nx) as usize];
                        r += px[0] as u32;
                        g += px[1] as u32;
                        b += px[2] as u32;
                        count += 1;
                    }
                }
            }
            let idx = ((y as usize * size) + x as usize) * 4;
            out[idx] = (r / count) as u8;
            out[idx + 1] = (g / count) as u8;
            out[idx + 2] = (b / count) as u8;
            out[idx + 3] = 255;
        }
    }
    out
}

/// Returns (hue[0,360), sat[0,1], light[0,1]).
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    let delta = max - min;
    if delta.abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let h = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (h * 60.0, s, l)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sksl_compiles() {
        if let Err(error) = RuntimeEffect::make_for_shader(MESH_SKSL, None) {
            panic!("mesh SkSL failed to compile: {error}");
        }
    }

    #[test]
    fn uniform_block_is_16_bytes() {
        let effect = RuntimeEffect::make_for_shader(MESH_SKSL, None).unwrap();
        assert_eq!(
            effect.uniform_size(),
            16,
            "packed uniform bytes must match the SkSL uniform block"
        );
    }

    #[test]
    fn mesh_builds_from_art() {
        let pixels = vec![0xff_20_60_a0u32; 64 * 64];
        assert!(
            MeshGradient::new(&pixels, 64, 64, 12345).is_some(),
            "MeshGradient::new returned None for valid art"
        );
    }
}

/// `h` is normalized [0,1). Returns (r,g,b) in [0,1].
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s.abs() < 1e-6 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0))
}
