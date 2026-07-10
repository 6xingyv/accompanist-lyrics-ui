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
    canvas::SaveLayerRec,
    image_filters::{self, CropRect},
    runtime_effect::ChildPtr,
    vertices::VertexMode,
    AlphaType, BlendMode, Canvas, Color, ColorType, CubicResampler, Data, Image, ImageInfo, ISize,
    Matrix, Paint, Point, RuntimeEffect, SamplingOptions, Shader, TileMode, Vertices,
};

/// Subdivisions per Hermite patch. The GL reference uses 50; here the per-frame
/// breathing warp re-tessellates the vertex positions each frame, so this is kept
/// modest — the linear-filtered texture + Hermite colour interpolation stay smooth
/// at 20, and the deforming grid is cheap to rebuild.
const SUBDIVISIONS: usize = 20;
/// Processed-artwork texture edge (see [`process_bitmap`]). Matches the reference
/// `ImageUtils.processBitmap` 32×32 downscale.
const TEX_SIZE: i32 = 32;

/// Ported `MeshShaders.FRAGMENT_SHADER`. `drawVertices` feeds the interpolated
/// mesh UV (`v_t`) in as the shader coordinate, so the album rotate/scale, vignette
/// and dither all key off it; the per-vertex mesh colour is multiplied in via
/// `drawVertices(Modulate)`. The GL vertex-stage breathing is applied on the
/// geometry (see [`MeshGradient::draw`]), not here. The result is returned OPAQUE:
/// the reference disables depth-test/cull and blends, but a deformed mesh folds over
/// itself, and blending those overlaps double-darkens the seams — an opaque result
/// makes overlapping triangles simply overwrite (painter's order), so the fade is
/// baked into RGB instead of alpha.
const MESH_SKSL: &str = r#"
uniform shader album;
uniform float uTexSize;
uniform float uTime;
uniform float uAmp;
uniform float uAlpha;

half4 main(float2 uv) {
    float vol = uAmp * 2.0;
    float t = uTime + uAmp;

    // Rotate + scale the texture sample around the centre (GL fragment stage).
    float2 c = uv - 0.5;
    float s = max(0.001, 1.0 - vol);
    float ca = cos(t * 2.0);
    float sa = sin(t * 2.0);
    float2 r = float2(ca * c.x - sa * c.y, sa * c.x + ca * c.y);
    float2 tuv = r * s + 0.5;

    float4 col = float4(album.eval(tuv * uTexSize));

    // alphaVolumeFactor baked into RGB (opaque output — see the doc comment).
    float alphaFactor = uAlpha * clamp(1.0 - uAmp * 0.5, 0.5, 1.0);
    col.rgb *= alphaFactor;

    // Hash-based dither to break up banding.
    float2 hp = fract(uv * float2(123.34, 456.21));
    hp += dot(hp, hp + 45.32);
    float dither = (fract(hp.x * hp.y) - 0.5) * (1.0 / 255.0);
    col.rgb += dither;

    // Vignette.
    float vig = smoothstep(0.8, 0.3, length(uv - 0.5));
    col.rgb *= (0.6 + 0.4 * vig);

    return half4(col.rgb, 1.0);
}
"#;

/// A GPU mesh-gradient built from one piece of artwork. The tessellation (base
/// positions in `p*1.4` mesh space, UVs, per-vertex colours, triangle indices) is
/// computed once; each frame the positions are re-breathed on the CPU (the GL
/// vertex shader) and drawn with the per-fragment `RuntimeEffect`.
pub struct MeshGradient {
    effect: RuntimeEffect,
    album: Shader,
    /// The original (unprocessed) artwork, for the player top-bar thumbnail.
    thumbnail: Image,
    /// The processed 32×32 RGBA artwork, kept so the control-point grid can be
    /// re-tessellated (and re-coloured) if the surface size/aspect changes.
    processed: Vec<u8>,
    /// Deterministic control-point layout seed (stable per song).
    seed: u32,
    /// Currently-built control-point grid `(cols, rows)`.
    grid: (usize, usize),
    /// Per-frame breathing amplitude (clip units), scaled to the grid so a denser
    /// grid never breathes a cell over itself.
    breath_amp: f32,
    /// Base vertex positions after `×1.4` (mesh/clip space), before breathing.
    base: Vec<Point>,
    texs: Vec<Point>,
    colors: Vec<Color>,
    indices: Vec<u16>,
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
        let Some(thumbnail) = make_thumbnail_image(pixels, width, height) else {
            log::warn!("[mesh] thumbnail image build failed");
            return None;
        };

        // Tessellate for a neutral phone-ish surface up front so the first frame has
        // geometry; `ensure_grid` re-tessellates once the real surface size is known.
        let (cols, rows) = desired_grid(1080.0, 1920.0);
        let Some((base, texs, colors, indices)) = build_mesh_arrays(seed, cols, rows, &processed)
        else {
            log::warn!("[mesh] tessellation failed (grid {cols}x{rows})");
            return None;
        };

        Some(Self {
            effect,
            album,
            thumbnail,
            processed,
            seed,
            grid: (cols, rows),
            breath_amp: breath_amp_for(cols.max(rows)),
            base,
            texs,
            colors,
            indices,
        })
    }

    /// The original artwork as an image, for the top-bar thumbnail.
    pub fn thumbnail(&self) -> &Image {
        &self.thumbnail
    }

    /// Re-tessellate the control-point grid for the current surface size/aspect if
    /// it no longer matches what was built. The grid dimensions come from the
    /// surface's physical size and aspect (see [`desired_grid`]); this is cheap and
    /// only actually rebuilds on a genuine size change (rotation / resize), never
    /// per frame.
    pub fn ensure_grid(&mut self, width: f32, height: f32) {
        let (cols, rows) = desired_grid(width, height);
        if (cols, rows) == self.grid {
            return;
        }
        if let Some((base, texs, colors, indices)) =
            build_mesh_arrays(self.seed, cols, rows, &self.processed)
        {
            self.base = base;
            self.texs = texs;
            self.colors = colors;
            self.indices = indices;
            self.grid = (cols, rows);
            self.breath_amp = breath_amp_for(cols.max(rows));
        }
    }

    /// Draw the background across the whole `width`×`height` surface.
    ///
    /// `time` is the (loudness-paced) animation clock; `amp` is the reactive
    /// amplitude (the GL `u_amp`); `alpha` is a global fade (`u_alpha`). Per frame:
    /// re-breathe the geometry (GL vertex shader), then one `drawVertices` with the
    /// per-fragment `RuntimeEffect`, wrapped in a slightly-blurred layer.
    pub fn draw(&self, canvas: &Canvas, width: f32, height: f32, time: f32, amp: f32, alpha: f32) {
        if width <= 0.0 || height <= 0.0 || self.base.is_empty() {
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

        // GL vertex-stage breathing: perturb each (already ×1.4) position by slow
        // sine/cosine waves so the whole gradient undulates organically.
        let breath_t = time * 0.5;
        let positions: Vec<Point> = self
            .base
            .iter()
            .map(|p| {
                let off_x = (p.y * 3.0 + breath_t).sin() * self.breath_amp;
                let off_y = (p.x * 2.5 + breath_t * 1.2).cos() * self.breath_amp;
                Point::new(p.x + off_x, p.y + off_y)
            })
            .collect();
        let vertices = Vertices::new_copy(
            VertexMode::Triangles,
            &positions,
            &self.texs,
            &self.colors,
            Some(&self.indices),
        );

        let mut paint = Paint::default();
        paint.set_anti_alias(false);
        paint.set_shader(shader);

        // "稍稍模糊": a light blur over the whole background softens the gradient and
        // hides any residual fold seams from the deforming mesh.
        let sigma = (width.min(height) * 0.005).clamp(2.0, 12.0);
        let mut layer_paint = Paint::default();
        if let Some(filter) = image_filters::blur((sigma, sigma), None, None, CropRect::NO_CROP_RECT)
        {
            layer_paint.set_image_filter(filter);
        }
        canvas.save_layer(&SaveLayerRec::default().paint(&layer_paint));
        // Positions are in `p*1.4` space (breathing already applied); the matrix only
        // does aspect correction + clip→pixel.
        canvas.concat(&mesh_to_pixel_matrix(width, height));
        // Modulate = multiply: shader_output * per-vertex mesh colour, matching the
        // GL `col.rgb *= v_c`.
        canvas.draw_vertices(&vertices, BlendMode::Modulate, &paint);
        canvas.restore();
    }
}

/// Affine map from breathed mesh space (positions already `×1.4`) to surface pixels,
/// replicating the GL vertex stage's aspect correction + the clip→viewport transform
/// (with Y flipped for Skia's Y-down canvas). Each step is a per-axis scale/translate,
/// so the whole thing is one `Matrix`.
fn mesh_to_pixel_matrix(width: f32, height: f32) -> Matrix {
    let aspect = width / height;
    let (aspect_x, aspect_y) = if aspect > 1.0 {
        (1.0, aspect)
    } else {
        (1.0 / aspect, 1.0)
    };
    let sx = aspect_x * width * 0.5;
    let sy = -aspect_y * height * 0.5; // negative: clip +Y (up) → screen −Y
    let tx = width * 0.5;
    let ty = height * 0.5;
    Matrix::new_all(sx, 0.0, tx, 0.0, sy, ty, 0.0, 0.0, 1.0)
}

/// Build a full-resolution image of the original artwork (ARGB_8888 → RGBA8888) for
/// the top-bar thumbnail.
fn make_thumbnail_image(pixels: &[u32], width: usize, height: usize) -> Option<Image> {
    let mut rgba = Vec::with_capacity(width * height * 4);
    for &argb in &pixels[..width * height] {
        rgba.push(((argb >> 16) & 0xff) as u8); // R
        rgba.push(((argb >> 8) & 0xff) as u8); // G
        rgba.push((argb & 0xff) as u8); // B
        rgba.push(((argb >> 24) & 0xff) as u8); // A
    }
    let info = ImageInfo::new(
        ISize::new(width as i32, height as i32),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    Image::from_raster_data(&info, Data::new_copy(&rgba), width * 4)
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
    // GL_MIRRORED_REPEAT, but Mitchell BICUBIC (not bilinear) filtering. The source
    // is only 32×32, so it is stretched ~30× across the surface; bilinear upsampling
    // is piecewise-linear and leaves visible Mach-band "stripes" at every texel
    // boundary. A Mitchell cubic (B=C=1/3) is C¹-smooth and ringing-free, so the
    // gradient reads as continuous with no texel grid. The shader evaluates it at
    // `uv * TEX_SIZE`, so no local matrix is needed.
    image.to_shader(
        (TileMode::Mirror, TileMode::Mirror),
        SamplingOptions::from(CubicResampler::mitchell()),
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

/// Control-point grid dimensions `(cols, rows)` for a surface, derived from its
/// physical size and aspect. The aspect correction in [`mesh_to_pixel_matrix`] maps
/// the clip-space mesh to a CENTRED SQUARE of side `1.4 · max(width, height)` (its
/// `|sx| == |sy|` always), so uniform, square screen-space cells — the look we want
/// — need an equal column and row count. That count scales with the longer edge, so
/// a bigger / higher-DPI surface gets a finer grid; it is clamped so the (one-time)
/// tessellation stays cheap. This is what makes the grid track the screen instead of
/// the old fixed random 4–6 × 4–6.
fn desired_grid(width: f32, height: f32) -> (usize, usize) {
    // ~one control point per this many render px along the mesh square's edge.
    const TARGET_CELL_PX: f32 = 700.0;
    let longer = width.max(height).max(1.0);
    let n = (1.0 + 1.4 * longer / TARGET_CELL_PX)
        .round()
        .clamp(4.0, 7.0) as usize;
    (n, n)
}

/// Per-frame breathing amplitude (in the `×1.4` base space) for a grid of `n` points
/// per axis, scaled so a denser grid (smaller cells) breathes proportionally less
/// and never folds a cell over. Capped at the original coarse-grid value so sparse
/// grids keep the same organic motion as before.
fn breath_amp_for(n: usize) -> f32 {
    let spacing = 2.8 / (n.max(2) as f32 - 1.0); // cell size in the ×1.4 base space
    (spacing * 0.12).min(0.05)
}

/// Build the tessellated mesh arrays for a `cols × rows` control-point grid.
fn build_mesh_arrays(
    seed: u32,
    cols: usize,
    rows: usize,
    processed: &[u8],
) -> Option<MeshArrays> {
    let preset = generate_control_points(seed, cols, rows);
    let control_points = control_points_from_preset(&preset, processed);
    tessellate(&preset, &control_points)
}

fn generate_control_points(seed: u32, w: usize, h: usize) -> Preset {
    let mut rng = Rng::new(seed);

    // Both the noise displacement and the Hermite tangent magnitudes are scaled to
    // the grid cell size so neither a control point nor a patch's curve can reach
    // into a neighbouring cell. Cell inversion is exactly what folds the mesh over
    // itself and shows up as hard triangle facets, and it got worse as the grid got
    // denser under the old fixed `strength = 0.5` / tangent range `0.5..1.5` (both
    // far larger than a cell). `strength` keeps a point near its cell centre; the
    // tangents stay Catmull-Rom sized (≈ one cell) so a patch doesn't overshoot.
    let cell = 2.0 / (w.max(h).max(2) as f32 - 1.0);
    let strength = cell * 0.35;

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
            let up = if is_border { cell } else { cell * rng.range(0.6, 1.0) };
            let vp = if is_border { cell } else { cell * rng.range(0.6, 1.0) };

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
    // u-tangent at corner (0,1). With only two columns of samples this is the same
    // forward difference as `tan_u00`; it was `tan_v01` — a stray v-tangent that
    // warped the top edge's colour interpolation into corner artifacts.
    m[9] = tan_u00;
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

/// Tessellate the patch grid into `(base_positions, texs, colors, indices)`. Base
/// positions are the surface points scaled `×1.4` (mesh/clip space, before the
/// per-frame breathing); UVs in [0, 1]; per-vertex colour Hermite-interpolated from
/// the artwork. Ported from `BHPMesh.update` + `generateIndices`.
type MeshArrays = (Vec<Point>, Vec<Point>, Vec<Color>, Vec<u16>);

fn tessellate(preset: &Preset, control_points: &[Vec<ControlPoint>]) -> Option<MeshArrays> {
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
            // Bake the GL vertex-shader `p * 1.4` here so the base is in the space
            // where breathing (± 0.05) and the aspect matrix operate.
            positions.push(Point::new(px * 1.4, py * 1.4));

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
    // realistic 6×6 control grid stays comfortably under.
    if num_vertices > u16::MAX as usize {
        return None;
    }

    Some((positions, texs, colors, indices))
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
