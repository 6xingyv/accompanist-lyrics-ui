use cosmic_text::fontdb;
use skia_safe::{
    canvas::SaveLayerRec,
    font,
    font_arguments::{variation_position::Coordinate, VariationPosition},
    gradient,
    image_filters::{self, CropRect},
    BlendMode, BlurStyle, Color4f, Font, FontArguments, FontHinting, FourByteTag, GlyphId, Image,
    MaskFilter, Paint, Point, RRect, Rect, SamplingOptions, Shader, TileMode, Typeface,
};
use std::collections::HashMap;
use std::f32::consts::PI;

use super::*;

/// Draw the player top bar (album thumbnail + title/artist + ⋯ button) inside the
/// surface. The thumbnail is a normal rounded-rect image; the title/artist and
/// button are composited additively (`Plus`) — the GPU equivalent of the old
/// Compose `graphicsLayer { blendMode = Plus }` metadata/controls.
pub(super) fn draw_top_bar_skia(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    thumbnail: Option<&Image>,
    bar: &PreparedTopBar,
    current_time_ms: i32,
) -> bool {
    // Thumbnail (normal blend), clipped to a rounded rectangle.
    if let Some(image) = thumbnail {
        let dst = Rect::new(
            bar.thumb_left,
            bar.thumb_top,
            bar.thumb_left + bar.thumb_size,
            bar.thumb_top + bar.thumb_size,
        );
        let rrect = RRect::new_rect_xy(dst, bar.thumb_radius, bar.thumb_radius);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        canvas.save();
        canvas.clip_rrect(rrect, None, true);
        canvas.draw_image_rect_with_sampling_options(
            image,
            None,
            dst,
            SamplingOptions::from(skia_safe::sampling_options::FilterMode::Linear),
            &paint,
        );
        canvas.restore();
    }

    // Title / artist / button — additive.
    let white = (255u8, 255u8, 255u8, 255u8);
    let mut plus_paint = Paint::default();
    plus_paint.set_blend_mode(BlendMode::Plus);
    canvas.save_layer(&SaveLayerRec::default().paint(&plus_paint));

    // Title / artist marquee: when a line is wider than its column it scrolls
    // toward the Start edge into the blank padding, its clip edges softened by a
    // fade, easing at each end of the travel (see `draw_top_bar_marquee_line`).
    let mut animating = false;
    animating |= draw_top_bar_marquee_line(
        canvas,
        typefaces,
        &bar.title,
        bar.text_left,
        bar.title_top,
        bar.text_max_width,
        white,
        1.0,
        current_time_ms,
    );
    animating |= draw_top_bar_marquee_line(
        canvas,
        typefaces,
        &bar.artist,
        bar.text_left,
        bar.artist_top,
        bar.text_max_width,
        white,
        bar.artist_alpha,
        current_time_ms,
    );

    // ⋯ button: a faint circle background + three dots.
    let mut bg = Paint::default();
    bg.set_anti_alias(true);
    bg.set_color4f(Color4f::new(1.0, 1.0, 1.0, 0.1), None);
    canvas.draw_circle(
        Point::new(bar.button_cx, bar.button_cy),
        bar.button_radius,
        &bg,
    );
    let mut dot = Paint::default();
    dot.set_anti_alias(true);
    dot.set_color4f(Color4f::new(1.0, 1.0, 1.0, 1.0), None);
    let dot_r = (bar.button_radius * 0.11).max(1.0);
    let spacing = bar.button_radius * 0.44;
    for k in -1i32..=1 {
        canvas.draw_circle(
            Point::new(bar.button_cx + k as f32 * spacing, bar.button_cy),
            dot_r,
            &dot,
        );
    }

    canvas.restore();
    animating
}

// Top-bar marquee tuning.
/// Width (px) of the soft fade at each edge of a marquee line's column.
const MARQUEE_FADE_PX: f32 = 24.0;
/// Scroll speed of the marquee travel, in px per second.
const MARQUEE_SCROLL_PX_PER_SEC: f32 = 40.0;
/// How long the text pauses (ms) at each end of the travel before easing back.
const MARQUEE_HOLD_MS: f32 = 1600.0;

/// Draw one top-bar text line (title or artist), marqueeing it when it overflows
/// its `max_width` column. The line is isolated in a layer clipped to the column;
/// an overflowing line is shifted toward the Start edge by an eased ping-pong
/// offset and its left/right edges are dissolved with a fade so the text slides
/// out of / into the surrounding padding cleanly. Returns whether it is animating
/// (i.e. actually overflowing) so the caller can keep the render loop ticking.
#[allow(clippy::too_many_arguments)]
fn draw_top_bar_marquee_line(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    text: &PreparedText,
    left: f32,
    top: f32,
    max_width: f32,
    color: (u8, u8, u8, u8),
    alpha: f32,
    current_time_ms: i32,
) -> bool {
    if alpha <= 0.0 || max_width <= 0.0 {
        return false;
    }

    let text_width = prepared_text_width(text);
    let overflow = text_width - max_width;
    let animating = overflow > 0.5;
    let offset = if animating {
        marquee_offset(current_time_ms, overflow)
    } else {
        0.0
    };

    // Isolate the line so the edge fade only dissolves this text, not the thumbnail
    // or button. Vertical bounds are padded generously around the text box.
    let top_bound = top - text.height * 0.5;
    let bottom_bound = top + text.height * 1.5;
    let bounds = Rect::new(left, top_bound, left + max_width, bottom_bound);
    canvas.save_layer(&SaveLayerRec::default().bounds(&bounds));

    canvas.save();
    canvas.clip_rect(bounds, skia_safe::ClipOp::Intersect, false);
    draw_prepared_text_skia(
        canvas,
        typefaces,
        text,
        left - offset,
        top,
        color,
        alpha,
        0.0,
        None,
    );
    canvas.restore();

    if animating {
        apply_horizontal_edge_fade(
            canvas,
            left,
            top_bound,
            max_width,
            bottom_bound - top_bound,
            MARQUEE_FADE_PX,
        );
    }

    canvas.restore();
    animating
}

/// Eased ping-pong marquee offset (px, toward the Start edge) for the given clock.
/// The text holds at the start, eases out to reveal its tail (travelling a bit past
/// the overflow so the last glyph clears the fade), holds, then eases back — so both
/// the start and end of every move are smooth rather than a constant-speed crawl.
fn marquee_offset(current_time_ms: i32, overflow: f32) -> f32 {
    let travel = overflow + MARQUEE_FADE_PX;
    let scroll_ms = (travel / MARQUEE_SCROLL_PX_PER_SEC * 1000.0).max(1.0);
    let hold_ms = MARQUEE_HOLD_MS;
    let period = 2.0 * (scroll_ms + hold_ms);
    let t = (current_time_ms as f32).rem_euclid(period);

    let phase = if t < hold_ms {
        0.0
    } else if t < hold_ms + scroll_ms {
        smooth_step((t - hold_ms) / scroll_ms)
    } else if t < 2.0 * hold_ms + scroll_ms {
        1.0
    } else {
        1.0 - smooth_step((t - 2.0 * hold_ms - scroll_ms) / scroll_ms)
    };
    phase * travel
}

/// Multiply the alpha of `[left, left+width]` by a horizontal gradient that fades to
/// transparent over `fade_px` at each edge (`DstIn`), softening a marquee line's
/// clip edges so the text dissolves into the padding instead of hard-clipping.
fn apply_horizontal_edge_fade(
    canvas: &skia_safe::Canvas,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    fade_px: f32,
) {
    if width <= 0.0 || height <= 0.0 || fade_px <= 0.0 {
        return;
    }
    let f = (fade_px / width).clamp(0.0, 0.5);
    let clear = Color4f::new(1.0, 1.0, 1.0, 0.0);
    let solid = Color4f::new(1.0, 1.0, 1.0, 1.0);
    let colors = [clear, solid, solid, clear];
    let positions = [0.0, f, 1.0 - f, 1.0];
    let gradient_colors = gradient::Colors::new(&colors, Some(&positions), TileMode::Clamp, None);
    let gradient = gradient::Gradient::new(gradient_colors, gradient::Interpolation::default());
    let Some(shader) = gradient::shaders::linear_gradient(
        (Point::new(left, 0.0), Point::new(left + width, 0.0)),
        &gradient,
        None,
    ) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_anti_alias(false);
    paint.set_shader(shader);
    paint.set_blend_mode(BlendMode::DstIn);
    canvas.draw_rect(Rect::new(left, top, left + width, top + height), &paint);
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SkiaGlyphBatchKey {
    font_id: fontdb::ID,
    font_size_bits: u32,
    alpha_bits: u32,
    weight: u16,
}

struct SkiaGlyphBatch {
    key: SkiaGlyphBatchKey,
    glyphs: Vec<GlyphId>,
    positions: Vec<Point>,
}

#[derive(Default)]
struct SkiaGlyphBatcher {
    batches: Vec<SkiaGlyphBatch>,
}

impl SkiaGlyphBatcher {
    fn push(&mut self, glyph: &PreparedGlyph, x: f32, y: f32, alpha: f32) {
        let cache_key = glyph.physical.cache_key;
        let key = SkiaGlyphBatchKey {
            font_id: cache_key.font_id,
            font_size_bits: cache_key.font_size_bits,
            alpha_bits: alpha.clamp(0.0, 1.0).to_bits(),
            weight: cache_key.font_weight.0,
        };

        if let Some(batch) = self.batches.iter_mut().find(|batch| batch.key == key) {
            batch.glyphs.push(cache_key.glyph_id);
            batch.positions.push(Point::new(x, y));
            return;
        }

        self.batches.push(SkiaGlyphBatch {
            key,
            glyphs: vec![cache_key.glyph_id],
            positions: vec![Point::new(x, y)],
        });
    }

    fn flush(
        &mut self,
        canvas: &skia_safe::Canvas,
        typefaces: &HashMap<fontdb::ID, Typeface>,
        base_color: (u8, u8, u8, u8),
        blur_radius: f32,
        karaoke_shader: Option<&Shader>,
    ) {
        for batch in self.batches.drain(..) {
            draw_skia_glyph_batch(
                canvas,
                typefaces,
                &batch,
                base_color,
                blur_radius,
                karaoke_shader,
            );
        }
    }
}

pub(super) fn draw_prepared_text_skia(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    text: &PreparedText,
    origin_x: f32,
    origin_y: f32,
    base_color: (u8, u8, u8, u8),
    alpha: f32,
    blur_radius: f32,
    karaoke: Option<(i32, bool, f32, &Vec<PreparedSyllable>)>,
) {
    // Out-of-focus lines blur as ONE gaussian layer (a single offscreen pass for
    // the whole line) instead of a `MaskFilter::blur` per glyph-batch (one pass
    // each) — the per-batch passes were what stalled the GPU mid-song. The layer
    // is bounded to the text plus the blur spread so the offscreen stays small.
    // `image_filters::blur` and `MaskFilter::blur` both take sigma, so the blur
    // amount is unchanged. Inner draws then run with zero blur (the layer does it).
    let layer_blur = blur_radius > 0.1;
    if layer_blur {
        // Bound the blur layer to where glyphs are ACTUALLY drawn. row.min_x/max_x
        // describe the text *width* but not its on-screen x for right-aligned rows
        // (plain text stores 0..line_w there while the glyphs sit at the right
        // margin) — using them put the layer on the left and clipped the
        // right-aligned translation to half. So take the left edge from the real
        // glyph positions and span by the row width (max_x - min_x).
        let mut left = f32::INFINITY;
        let mut text_width = 0.0f32;
        for row in &text.rows {
            text_width = text_width.max(row.max_x - row.min_x);
            for glyph in &row.glyphs {
                left = left.min(origin_x + glyph.physical.x as f32);
            }
        }
        if !left.is_finite() {
            left = origin_x;
            text_width = text.rows.first().map(|row| row.width).unwrap_or(0.0);
        }
        // Pad covers the blur spread (~3 sigma) plus a glyph's worth of overhang
        // past its pen origin on the right.
        let pad = blur_radius * 3.0 + text.height.max(4.0);
        let bounds = Rect::new(
            left - pad,
            origin_y - pad,
            left + text_width + pad,
            origin_y + text.height + pad,
        );
        let mut layer_paint = Paint::default();
        if let Some(filter) = image_filters::blur(
            (blur_radius, blur_radius),
            None,
            None,
            CropRect::NO_CROP_RECT,
        ) {
            layer_paint.set_image_filter(filter);
        }
        canvas.save_layer(&SaveLayerRec::default().bounds(&bounds).paint(&layer_paint));
    }
    let inner_blur = if layer_blur { 0.0 } else { blur_radius };

    for row in &text.rows {
        let (row_min_x, row_max_x) =
            row_x_bounds(row, origin_x).unwrap_or((origin_x, origin_x + row.width));
        let active_edge = karaoke.and_then(|(time, is_rtl, _inactive_alpha, syllables)| {
            active_edge_for_row(row, origin_x, time, is_rtl, syllables)
        });
        let karaoke_brush = active_edge.map(|active_edge| KaraokeBrush {
            active_edge,
            row_min_x,
            row_max_x,
            is_rtl: karaoke.map(|(_, is_rtl, _, _)| is_rtl).unwrap_or(false),
            inactive_alpha: karaoke
                .map(|(_, _, inactive_alpha, _)| inactive_alpha)
                .unwrap_or(KARAOKE_INACTIVE_ALPHA),
        });
        let karaoke_shader = karaoke_brush.and_then(|brush| make_karaoke_shader(brush, base_color));
        let mut batcher = SkiaGlyphBatcher::default();
        let can_batch_brush = karaoke_shader.is_some() || karaoke_brush.is_none();

        for (glyph_position, glyph) in row.glyphs.iter().enumerate() {
            let effect = karaoke
                .and_then(|(time, _, _, syllables)| {
                    glyph_effect_for_time(glyph, row, glyph_position, time, syllables)
                })
                .unwrap_or_default();
            let glyph_alpha = alpha * glyph.alpha_multiplier;
            if glyph_alpha <= 0.0 {
                continue;
            }

            let x = glyph.physical.x as f32 + origin_x;
            let y = glyph.physical.y as f32 + origin_y + stable_animation_offset(effect.offset_y);
            let has_dynamic_transform =
                (effect.scale - 1.0).abs() > 0.001 || effect.shadow_blur_radius > 0.1;

            if !has_dynamic_transform && can_batch_brush {
                batcher.push(glyph, x, y, glyph_alpha);
                continue;
            }

            batcher.flush(
                canvas,
                typefaces,
                base_color,
                inner_blur,
                karaoke_shader.as_ref(),
            );

            let scale_pivot = effect
                .scale_pivot
                .map(|(x, y)| (origin_x + x, origin_y + y));

            if effect.shadow_blur_radius > 0.1 && !glyph.is_phonetic {
                // `Outer` blur keeps the glow strictly outside the glyph
                // silhouette, so it never fills (and additively brightens) the
                // interior of semi-transparent text; it still rides the same
                // scale/pivot and the karaoke brush/shader, so it follows the
                // syllable swell and the fill sweep.
                draw_skia_glyph(
                    canvas,
                    typefaces,
                    glyph,
                    x,
                    y,
                    base_color,
                    glyph_alpha * 0.4,
                    effect.shadow_blur_radius,
                    BlurStyle::Outer,
                    effect.scale,
                    scale_pivot,
                    karaoke_brush,
                    karaoke_shader.as_ref(),
                );
            }

            draw_skia_glyph(
                canvas,
                typefaces,
                glyph,
                x,
                y,
                base_color,
                glyph_alpha,
                inner_blur,
                BlurStyle::Normal,
                effect.scale,
                scale_pivot,
                karaoke_brush,
                karaoke_shader.as_ref(),
            );
        }
        batcher.flush(
            canvas,
            typefaces,
            base_color,
            inner_blur,
            karaoke_shader.as_ref(),
        );
    }

    if layer_blur {
        canvas.restore();
    }
}

fn draw_skia_glyph_batch(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    batch: &SkiaGlyphBatch,
    base_color: (u8, u8, u8, u8),
    blur_radius: f32,
    karaoke_shader: Option<&Shader>,
) {
    let font_size = f32::from_bits(batch.key.font_size_bits).max(1.0);
    let alpha = f32::from_bits(batch.key.alpha_bits).clamp(0.0, 1.0);
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    if let Some(shader) = karaoke_shader {
        paint.set_color4f(Color4f::new(1.0, 1.0, 1.0, 1.0), None);
        paint.set_alpha_f(alpha);
        paint.set_shader(Some(Shader::clone(shader)));
    } else {
        paint.set_color4f(skia_color(base_color, alpha), None);
    }
    if blur_radius > 0.1 {
        if let Some(mask_filter) = MaskFilter::blur(BlurStyle::Normal, blur_radius, true) {
            paint.set_mask_filter(mask_filter);
        }
    }

    with_skia_font(
        batch.key.font_id,
        font_size,
        batch.key.weight,
        typefaces,
        |font| {
            canvas.draw_glyphs_at(
                &batch.glyphs,
                &batch.positions[..],
                Point::new(0.0, 0.0),
                font,
                &paint,
            );
        },
    );
}

fn draw_skia_glyph(
    canvas: &skia_safe::Canvas,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    glyph: &PreparedGlyph,
    x: f32,
    y: f32,
    base_color: (u8, u8, u8, u8),
    alpha: f32,
    blur_radius: f32,
    blur_style: BlurStyle,
    scale: f32,
    scale_pivot: Option<(f32, f32)>,
    karaoke_brush: Option<KaraokeBrush>,
    karaoke_shader: Option<&Shader>,
) {
    let cache_key = glyph.physical.cache_key;
    let font_size = f32::from_bits(cache_key.font_size_bits).max(1.0);

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    if let Some(shader) = karaoke_shader {
        paint.set_color4f(Color4f::new(1.0, 1.0, 1.0, 1.0), None);
        paint.set_alpha_f(alpha.clamp(0.0, 1.0));
        paint.set_shader(Some(Shader::clone(shader)));
    } else {
        let brush_alpha = karaoke_brush
            .map(|brush| brush.sample_alpha(x))
            .unwrap_or(1.0);
        paint.set_color4f(skia_color(base_color, alpha * brush_alpha), None);
    }
    if blur_radius > 0.1 {
        if let Some(mask_filter) = MaskFilter::blur(blur_style, blur_radius, true) {
            paint.set_mask_filter(mask_filter);
        }
    }

    let glyphs: [GlyphId; 1] = [cache_key.glyph_id];
    let positions = [Point::new(x, y)];
    with_skia_font(
        cache_key.font_id,
        font_size,
        cache_key.font_weight.0,
        typefaces,
        |font| {
            if (scale - 1.0).abs() > 0.001 {
                let (pivot_x, pivot_y) = scale_pivot.unwrap_or((x, y));
                canvas.save();
                canvas.translate((pivot_x, pivot_y));
                canvas.scale((scale, scale));
                canvas.translate((-pivot_x, -pivot_y));
                canvas.draw_glyphs_at(&glyphs, &positions[..], Point::new(0.0, 0.0), font, &paint);
                canvas.restore();
            } else {
                canvas.draw_glyphs_at(&glyphs, &positions[..], Point::new(0.0, 0.0), font, &paint);
            }
        },
    );
}

/// A Skia `Font` is immutable for a given (typeface, size), so build each one
/// once and reuse it across batches and frames instead of reconstructing it
/// (an allocation plus six setters) on every draw call. Keyed by the
/// process-global Skia typeface id, so the cache stays correct even when several
/// renderer instances share the (single) render thread — unlike `fontdb::ID`,
/// which is only unique within one renderer. The font is handed to `f` by
/// reference and never cloned; `None` if the typeface isn't resolved yet.
fn with_skia_font<R>(
    font_id: fontdb::ID,
    font_size: f32,
    weight: u16,
    typefaces: &HashMap<fontdb::ID, Typeface>,
    f: impl FnOnce(&Font) -> R,
) -> Option<R> {
    thread_local! {
        static FONT_CACHE: std::cell::RefCell<HashMap<(u32, u32, u16), Font>> =
            std::cell::RefCell::new(HashMap::new());
    }
    let typeface = typefaces.get(&font_id)?;
    let key = (typeface.unique_id(), font_size.to_bits(), weight);
    Some(FONT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let font = cache
            .entry(key)
            .or_insert_with(|| make_skia_font(typeface_at_weight(typeface, weight), font_size));
        f(font)
    }))
}

/// cosmic-text shapes variable fonts at the requested weight by setting the
/// `wght` axis (see its `font/mod.rs`), so the glyph *advances* are already for
/// that weight. Skia, though, draws the typeface's default instance (usually
/// 400) unless we set the same axis — that mismatch is the "measured bold, drawn
/// thin" bug. Clone the typeface at the requested `wght`; a no-op for static
/// fonts (no `wght` axis), and `clone` failures fall back to the original.
fn typeface_at_weight(typeface: &Typeface, weight: u16) -> Typeface {
    let coordinates = [Coordinate {
        axis: FourByteTag::from_chars('w', 'g', 'h', 't'),
        value: weight as f32,
    }];
    let arguments = FontArguments::new().set_variation_design_position(VariationPosition {
        coordinates: &coordinates,
    });
    typeface
        .clone_with_arguments(&arguments)
        .unwrap_or_else(|| typeface.clone())
}

fn make_skia_font(typeface: Typeface, font_size: f32) -> Font {
    let mut font = Font::new(typeface, font_size);
    font.set_subpixel(true);
    font.set_linear_metrics(true);
    font.set_baseline_snap(false);
    font.set_embedded_bitmaps(false);
    font.set_hinting(FontHinting::None);
    font.set_edging(font::Edging::SubpixelAntiAlias);
    font
}

fn stable_animation_offset(value: f32) -> f32 {
    (value * 64.0).round() / 64.0
}

fn skia_color(color: (u8, u8, u8, u8), alpha: f32) -> Color4f {
    let a = (color.3 as f32 / 255.0) * alpha.clamp(0.0, 1.0);
    Color4f::new(
        color.0 as f32 / 255.0,
        color.1 as f32 / 255.0,
        color.2 as f32 / 255.0,
        a,
    )
}

/// Multiply the canvas's alpha by a vertical gradient so the top and bottom edges
/// of the band `[y_top, y_bottom]` fade to transparent — the GPU-path equivalent of
/// [`apply_vertical_fade`]. Draws one band-bounds rect with `BlendMode::DstIn`
/// (result = dst × src.alpha). In the transparent-overlay mode `[y_top, y_bottom]`
/// is the whole surface; in full-bleed mode the caller isolates the lyrics in a
/// layer first so this only dissolves the lyrics, not the opaque background.
pub(super) fn apply_vertical_fade_skia_band(
    canvas: &skia_safe::Canvas,
    width: f32,
    y_top: f32,
    y_bottom: f32,
    top_px: f32,
    bottom_px: f32,
) {
    let band_height = y_bottom - y_top;
    if width <= 0.0 || band_height <= 0.0 || (top_px <= 0.0 && bottom_px <= 0.0) {
        return;
    }
    let top_stop = (top_px / band_height).clamp(0.0, 1.0);
    let bottom_stop = (1.0 - bottom_px / band_height).clamp(0.0, 1.0);
    // Only the alpha channel matters for DstIn; keep RGB at 1.
    let clear = Color4f::new(1.0, 1.0, 1.0, 0.0);
    let solid = Color4f::new(1.0, 1.0, 1.0, 1.0);
    // On a very short band the two fades could overlap (bottom_stop <= top_stop);
    // fall back to a single centre-peaked fade so it degrades gracefully.
    let (colors, positions): (Vec<Color4f>, Vec<f32>) = if bottom_stop <= top_stop {
        (vec![clear, solid, clear], vec![0.0, 0.5, 1.0])
    } else {
        (
            vec![clear, solid, solid, clear],
            vec![0.0, top_stop, bottom_stop, 1.0],
        )
    };
    let gradient_colors = gradient::Colors::new(&colors, Some(&positions), TileMode::Clamp, None);
    let gradient = gradient::Gradient::new(gradient_colors, gradient::Interpolation::default());
    let Some(shader) = gradient::shaders::linear_gradient(
        (Point::new(0.0, y_top), Point::new(0.0, y_bottom)),
        &gradient,
        None,
    ) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_anti_alias(false);
    paint.set_shader(shader);
    paint.set_blend_mode(BlendMode::DstIn);
    canvas.draw_rect(Rect::new(0.0, y_top, width, y_bottom), &paint);
}

fn make_karaoke_shader(brush: KaraokeBrush, base_color: (u8, u8, u8, u8)) -> Option<Shader> {
    let (fade_start, fade_end) = brush.fade_bounds();
    if fade_end <= fade_start {
        return None;
    }

    let active = skia_color(base_color, 1.0);
    let inactive = skia_color(base_color, brush.inactive_alpha);
    let colors = if brush.is_rtl {
        [inactive, active]
    } else {
        [active, inactive]
    };
    let positions = [0.0, 1.0];
    let gradient_colors = gradient::Colors::new(&colors, Some(&positions), TileMode::Clamp, None);
    let gradient = gradient::Gradient::new(gradient_colors, gradient::Interpolation::default());
    gradient::shaders::linear_gradient(
        (Point::new(fade_start, 0.0), Point::new(fade_end, 0.0)),
        &gradient,
        None,
    )
}

fn glyph_effect_for_time(
    glyph: &PreparedGlyph,
    row: &PreparedRow,
    glyph_position: usize,
    current_time_ms: i32,
    syllables: &[PreparedSyllable],
) -> Option<GlyphRenderEffect> {
    let index = glyph.syllable_index?;
    let syllable = syllables.get(index)?;
    if syllable.use_awesome {
        return Some(awesome_glyph_effect(glyph, current_time_ms, syllable));
    }

    let driver = simple_animation_driver(index, row, glyph_position, syllables);
    let duration = SIMPLE_ANIMATION_DURATION_MS;
    let progress = ((current_time_ms - driver.start) as f32 / duration).clamp(0.0, 1.0);
    Some(GlyphRenderEffect {
        offset_y: SIMPLE_LIFT_PX * cubic_bezier_easing(1.0 - progress, 0.0, 0.0, 0.2, 1.0),
        ..GlyphRenderEffect::default()
    })
}

fn simple_animation_driver<'a>(
    index: usize,
    row: &PreparedRow,
    glyph_position: usize,
    syllables: &'a [PreparedSyllable],
) -> &'a PreparedSyllable {
    let syllable = &syllables[index];
    if !is_punctuation_or_space(&syllable.content) {
        return syllable;
    }

    for candidate_glyph in row.glyphs[..glyph_position].iter().rev() {
        if let Some(candidate_index) = candidate_glyph.syllable_index {
            if let Some(candidate) = syllables.get(candidate_index) {
                if !is_punctuation_or_space(&candidate.content) {
                    return candidate;
                }
            }
        }
    }
    syllable
}

fn awesome_glyph_effect(
    glyph: &PreparedGlyph,
    current_time_ms: i32,
    syllable: &PreparedSyllable,
) -> GlyphRenderEffect {
    awesome_glyph_effect_for_char(glyph.animation_char_index, current_time_ms, syllable)
}

pub(super) fn awesome_glyph_effect_for_char(
    animation_char_index: f32,
    current_time_ms: i32,
    syllable: &PreparedSyllable,
) -> GlyphRenderEffect {
    let awesome_duration = (syllable.word_duration as f32 * AWESOME_DURATION_RATIO).max(1.0);
    let latest_start = syllable.word_end as f32 - awesome_duration;
    let absolute_char_index = syllable.char_offset_in_word as f32
        + animation_char_index.min(syllable.char_count.saturating_sub(1) as f32);
    let char_ratio = if syllable.word_char_count > 1 {
        absolute_char_index / (syllable.word_char_count - 1) as f32
    } else {
        0.5
    };
    let start_time =
        syllable.word_start as f32 + (latest_start - syllable.word_start as f32) * char_ratio;
    let progress = ((current_time_ms as f32 - start_time) / awesome_duration).clamp(0.0, 1.0);

    let spare_duration = (syllable.word_duration as f32
        - AWESOME_FAST_CHAR_THRESHOLD_MS * syllable.word_char_count as f32)
        .max(0.0);
    let dip = (0.5 * spare_duration / 1000.0).clamp(0.0, 0.5);
    let swell_amount = (0.1 * spare_duration / 1000.0).clamp(0.0, 0.1);

    GlyphRenderEffect {
        offset_y: AWESOME_LIFT_PX * dip_and_rise(1.0 - progress, dip, 1.0),
        scale: 1.0 + swell(progress, swell_amount),
        shadow_blur_radius: AWESOME_MAX_SHADOW_BLUR_PX * bounce(progress),
        scale_pivot: Some((syllable.word_pivot_x, syllable.word_pivot_y)),
    }
}

fn active_edge_for_row(
    row: &PreparedRow,
    origin_x: f32,
    current_time_ms: i32,
    is_rtl: bool,
    syllables: &[PreparedSyllable],
) -> Option<f32> {
    if row.glyphs.is_empty() {
        return None;
    }

    let (row_min_x, row_max_x) = row_x_bounds(row, origin_x)?;

    if current_time_ms <= row_first_time(row, syllables) {
        return Some(if is_rtl { row_max_x } else { row_min_x });
    }
    if current_time_ms >= row_last_time(row, syllables) {
        return Some(if is_rtl { row_min_x } else { row_max_x });
    }

    let mut edge = if is_rtl { row_max_x } else { row_min_x };
    let mut last_syllable_index = None;
    for glyph in &row.glyphs {
        if glyph.is_phonetic {
            continue;
        }
        let Some(index) = glyph.syllable_index else {
            continue;
        };
        if last_syllable_index == Some(index) {
            continue;
        }
        last_syllable_index = Some(index);
        let Some(syllable) = syllables.get(index) else {
            continue;
        };
        let left = origin_x + syllable.layout_x;
        let right = left + syllable.layout_width;
        if current_time_ms >= syllable.end {
            edge = if is_rtl { left } else { right };
        } else if current_time_ms >= syllable.start {
            let duration = (syllable.end - syllable.start).max(1) as f32;
            let progress = ((current_time_ms - syllable.start) as f32 / duration).clamp(0.0, 1.0);
            edge = if is_rtl {
                right - syllable.layout_width * progress
            } else {
                left + syllable.layout_width * progress
            };
            break;
        }
    }
    Some(edge)
}

fn row_x_bounds(row: &PreparedRow, origin_x: f32) -> Option<(f32, f32)> {
    if row.max_x <= row.min_x {
        return None;
    }
    Some((origin_x + row.min_x, origin_x + row.max_x))
}

fn row_first_time(row: &PreparedRow, syllables: &[PreparedSyllable]) -> i32 {
    row.glyphs
        .iter()
        .filter(|glyph| !glyph.is_phonetic)
        .filter_map(|glyph| glyph.syllable_index.and_then(|index| syllables.get(index)))
        .map(|syllable| syllable.start)
        .min()
        .unwrap_or(0)
}

fn row_last_time(row: &PreparedRow, syllables: &[PreparedSyllable]) -> i32 {
    row.glyphs
        .iter()
        .filter(|glyph| !glyph.is_phonetic)
        .filter_map(|glyph| glyph.syllable_index.and_then(|index| syllables.get(index)))
        .map(|syllable| syllable.end)
        .max()
        .unwrap_or(0)
}

pub(super) fn make_interlude_slot(
    line_index: usize,
    line_start: i32,
    previous_end: Option<i32>,
    right_aligned: bool,
    config: &SceneConfig,
) -> Option<PreparedInterlude> {
    let (start, end) = if line_index == 0 {
        if line_start > INTERLUDE_THRESHOLD_MS {
            (0, line_start)
        } else {
            return None;
        }
    } else {
        let previous_end = previous_end?;
        if line_start - previous_end > INTERLUDE_THRESHOLD_MS {
            (previous_end, line_start)
        } else {
            return None;
        }
    };

    let height = config.breathing_dots.size + DOTS_VERTICAL_PADDING * 2.0 + config.padding_y * 2.0;
    Some(PreparedInterlude {
        start,
        end,
        right_aligned,
        height,
    })
}

pub(super) fn draw_breathing_dots_skia(
    canvas: &skia_safe::Canvas,
    y: f32,
    interlude: &PreparedInterlude,
    config: &SceneConfig,
    current_time_ms: i32,
    line_alpha: f32,
) {
    let dots = config.breathing_dots;
    let total_width = dots_total_width(dots);
    if total_width <= 0.0 {
        return;
    }

    let origin_x = if interlude.right_aligned {
        config.width as f32 - config.content_right - config.padding_x - total_width
    } else {
        config.content_left + config.padding_x
    };
    let origin_y = y + config.padding_y;
    let (scale, alpha, enter_end, exit_start) =
        breathing_dots_state(interlude.start, interlude.end, current_time_ms, dots);
    if alpha <= 0.0 || scale <= 0.0 {
        return;
    }

    let color = rgba_from_argb(dots.color);
    let current_time = current_time_ms as f32;
    let center_x = origin_x + total_width * 0.5;
    let center_y = origin_y + dots.size * 0.5;
    // No brush/wipe reveal: the whole slot fades in/out on the master `alpha`
    // (enter 0->1, exit 1->0). Between enter-end and exit-start the dots simply
    // light up one after another (0.4 -> 1.0, staggered across that window).
    let light_window = (exit_start - enter_end).max(1.0);
    let dot_span = light_window / dots.number as f32;

    for index in 0..dots.number {
        let base_x = origin_x + dots.size * 0.5 + (dots.size + dots.margin) * index as f32;
        let base_y = origin_y + dots.size * 0.5;
        let scaled_x = center_x + (base_x - center_x) * scale;
        let scaled_y = center_y + (base_y - center_y) * scale;
        let radius = dots.size * 0.5 * scale;
        let dot_start = enter_end + dot_span * index as f32;
        let dot_alpha = 0.4 + 0.6 * ((current_time - dot_start) / dot_span).clamp(0.0, 1.0);

        let total_alpha = alpha * dot_alpha * line_alpha;
        if radius <= 0.0 || total_alpha <= 0.0 {
            continue;
        }

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(skia_color(color, total_alpha), None);
        canvas.draw_circle(Point::new(scaled_x, scaled_y), radius, &paint);
    }
}

/// Returns `(scale, alpha, enter_end_ms, exit_start_ms)`: the breathing scale and
/// the master fade envelope for the whole slot, plus the (variable-length-window
/// aware) instants when the enter fade finishes and the exit fade begins — the
/// caller lights the individual dots up across exactly that middle window.
fn breathing_dots_state(
    start_ms: i32,
    end_ms: i32,
    current_time_ms: i32,
    dots: BreathingDotsConfig,
) -> (f32, f32, f32, f32) {
    const ENTER_BREATH_SCALE: f32 = 0.8;
    const FULL_SCALE: f32 = 1.0;
    // If the gap is only barely longer than enter+dip+still+exit, a single
    // sub-frame "breath" from 0.8 to 1.0 reads as a pop. Treat that as no
    // breathing window and let enter finish at the same scale dip starts from.
    const MIN_VISIBLE_BREATHING_MS: f32 = 16.0;

    let start = start_ms as f32;
    let end = end_ms as f32;
    let current = current_time_ms as f32;
    let total_available = (end - start).max(1.0);
    let default_total = dots.enter_ms + dots.dip_ms + dots.still_ms + dots.exit_ms;
    let factor = if total_available < default_total {
        total_available / default_total
    } else {
        1.0
    };
    let enter = dots.enter_ms * factor;
    let dip = dots.dip_ms * factor;
    let still = dots.still_ms * factor;
    let exit = dots.exit_ms * factor;
    let enter_end = start + enter;
    let dip_start = end - exit - still - dip;
    let still_start = end - exit - still;
    let exit_start = end - exit;
    let breathing_duration = (dip_start - enter_end).max(0.0);
    let has_visible_breathing = breathing_duration > MIN_VISIBLE_BREATHING_MS;
    let enter_end_scale = if has_visible_breathing {
        ENTER_BREATH_SCALE
    } else {
        FULL_SCALE
    };

    if current < enter_end {
        let progress = ((current - start) / (enter_end - start).max(f32::EPSILON)).clamp(0.0, 1.0);
        let eased = smooth_step(progress);
        return (eased * enter_end_scale, eased, enter_end, exit_start);
    }
    if has_visible_breathing && current < dip_start {
        // Breathe at ~3000ms/cycle, but stretch the period slightly so a whole
        // number of HALF-cycles fits the (variable-length) breathing window and
        // it always ends at a peak (value 1.0) — exactly where the dip phase
        // begins. With the old fixed 3000ms period the window ended at an
        // arbitrary phase, leaving a leftover ~half oscillation that read as an
        // "extra half cycle" before the dip. `0.9 - 0.1·cos`: starts at 0.8
        // (matching the enter end), peaks at 1.0 every odd half-cycle.
        let half_cycles = {
            let n = (breathing_duration / 1500.0).round().max(1.0);
            if (n as i64) % 2 == 0 {
                n + 1.0
            } else {
                n
            }
        };
        let period = 2.0 * breathing_duration / half_cycles;
        let angle = ((current - enter_end) / period) * 2.0 * PI;
        return (0.9 - 0.1 * angle.cos(), 1.0, enter_end, exit_start);
    }
    if current < dip_start {
        return (FULL_SCALE, 1.0, enter_end, exit_start);
    }
    if current < still_start {
        let progress =
            ((current - dip_start) / (still_start - dip_start).max(f32::EPSILON)).clamp(0.0, 1.0);
        return (0.8 + 0.2 * (progress * 2.0 * PI).cos(), 1.0, enter_end, exit_start);
    }
    if current < exit_start {
        return (1.0, 1.0, enter_end, exit_start);
    }

    let progress = ((end - current) / (end - exit_start).max(f32::EPSILON)).clamp(0.0, 1.0);
    let eased = smooth_step(progress);
    (eased, eased, enter_end, exit_start)
}

fn dots_total_width(dots: BreathingDotsConfig) -> f32 {
    dots.size * dots.number as f32 + dots.margin * dots.number.saturating_sub(1) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dots() -> BreathingDotsConfig {
        BreathingDotsConfig {
            number: 3,
            size: 16.0,
            margin: 12.0,
            enter_ms: 3000.0,
            still_ms: 200.0,
            dip_ms: 3000.0,
            exit_ms: 200.0,
            color: 0xffff_ffff,
        }
    }

    fn max_adjacent_scale_jump(duration_ms: i32) -> (f32, i32, f32, f32) {
        let dots = test_dots();
        let mut previous = breathing_dots_state(0, duration_ms, 0, dots).0;
        let mut worst = (0.0, 0, previous, previous);
        for t in 1..=duration_ms {
            let scale = breathing_dots_state(0, duration_ms, t, dots).0;
            let jump = (scale - previous).abs();
            if jump > worst.0 {
                worst = (jump, t, previous, scale);
            }
            previous = scale;
        }
        worst
    }

    #[test]
    fn breathing_dots_scale_stays_continuous_when_breathing_window_is_missing() {
        // 6400ms is exactly enter+dip+still+exit. The old formula ended enter at
        // 0.8 then started dip at 1.0, producing a 0.2 scale pop at t=3000.
        let (jump, time_ms, before, after) = max_adjacent_scale_jump(6400);
        assert!(
            jump < 0.02,
            "scale jumped by {jump:.3} at {time_ms}ms ({before:.3} -> {after:.3})"
        );

        let enter_end_scale = breathing_dots_state(0, 6400, 3000, test_dots()).0;
        assert!((enter_end_scale - 1.0).abs() < 0.0001);
    }

    #[test]
    fn breathing_dots_scale_has_no_large_jumps_for_real_interlude_lengths() {
        for duration_ms in [5001, 6200, 6401, 6500, 8000, 12_345, 40_000] {
            let (jump, time_ms, before, after) = max_adjacent_scale_jump(duration_ms);
            assert!(
                jump < 0.05,
                "duration {duration_ms}ms jumped by {jump:.3} at {time_ms}ms ({before:.3} -> {after:.3})"
            );
        }

        let mut seed = 0x1234_5678u32;
        for _ in 0..256 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let duration_ms = 5001 + (seed % 35_000) as i32;
            let (jump, time_ms, before, after) = max_adjacent_scale_jump(duration_ms);
            assert!(
                jump < 0.05,
                "duration {duration_ms}ms jumped by {jump:.3} at {time_ms}ms ({before:.3} -> {after:.3})"
            );
        }
    }
}

fn smooth_step(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// Nested accompaniment lines bloom in over this window. It is matched to the
// auto-scroll spring's settle time so the entrance completes just as the main
// line reaches its target position, and it is anchored at the accompaniment's
// own start: a before-line accompaniment starts together with its cluster, so
// its bloom finishes as the main scrolls into place; an after-line one blooms in
// as it begins to be sung.
pub(super) const ACCOMPANIMENT_ENTER_MS: f32 = 500.0;

pub(super) fn accompaniment_visibility(start_ms: i32, end_ms: i32, current_time_ms: i32) -> f32 {
    // Grow the accompaniment into place from its start, hold, then ease it back
    // out after it ends. This one curve drives the make-room height, the line
    // alpha AND the scale bloom (so the appear and disappear animations match).
    // Kept short (and matched to the scroll spring's ~0.5s settle) so it
    // harmonizes with the auto-scroll instead of dragging on and overlapping the
    // next line's expand (which makes the focus bob when two consecutive lines
    // both carry an accompaniment).
    const EXIT_LINGER_MS: f32 = 200.0;
    const EXIT_FADE_MS: f32 = 400.0;

    let start = start_ms as f32;
    let end = end_ms as f32;
    let current = current_time_ms as f32;
    let exit_end = end + EXIT_LINGER_MS + EXIT_FADE_MS;
    if current < start || current > exit_end {
        return 0.0;
    }
    let enter = smooth_step((current - start) / ACCOMPANIMENT_ENTER_MS);
    let exit = smooth_step((exit_end - current) / EXIT_FADE_MS);
    enter.min(exit)
}

pub(super) fn interlude_visibility(start_ms: i32, end_ms: i32, current_time_ms: i32) -> f32 {
    // The breathing-dots slot opens and closes with a short deterministic ease so
    // it appears/leaves cleanly (no spring wobble, no hard layout pop) while the
    // neighbouring lines slide to make/fill the room.
    const FADE_MS: f32 = 220.0;

    let start = start_ms as f32;
    let end = end_ms as f32;
    let current = current_time_ms as f32;
    if current < start || current >= end {
        return 0.0;
    }
    let enter = smooth_step((current - start) / FADE_MS);
    let exit = smooth_step((end - current) / FADE_MS);
    enter.min(exit)
}

fn cubic_bezier_easing(fraction: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let x = fraction.clamp(0.0, 1.0);
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }

    let mut t = x;
    for _ in 0..8 {
        let current_x = cubic_bezier_value(t, x1, x2) - x;
        let derivative = cubic_bezier_derivative(t, x1, x2);
        if derivative.abs() < 1e-6 {
            break;
        }
        let next = t - current_x / derivative;
        if !(0.0..=1.0).contains(&next) {
            break;
        }
        t = next;
    }

    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..12 {
        let current_x = cubic_bezier_value(t, x1, x2);
        if (current_x - x).abs() < 1e-5 {
            break;
        }
        if current_x < x {
            low = t;
        } else {
            high = t;
        }
        t = (low + high) * 0.5;
    }

    cubic_bezier_value(t, y1, y2)
}

fn cubic_bezier_value(t: f32, control1: f32, control2: f32) -> f32 {
    let inv = 1.0 - t;
    3.0 * inv * inv * t * control1 + 3.0 * inv * t * t * control2 + t * t * t
}

fn cubic_bezier_derivative(t: f32, control1: f32, control2: f32) -> f32 {
    let inv = 1.0 - t;
    3.0 * inv * inv * control1
        + 6.0 * inv * t * (control2 - control1)
        + 3.0 * t * t * (1.0 - control2)
}

pub(super) fn dip_and_rise(fraction: f32, dip: f32, rise: f32) -> f32 {
    newton_interpolation_3(fraction, (0.0, 0.0), (0.5, -dip), (1.0, rise))
}

pub(super) fn swell(fraction: f32, amount: f32) -> f32 {
    newton_interpolation_3(fraction, (0.0, 0.0), (0.5, amount), (1.0, 0.0))
}

pub(super) fn bounce(fraction: f32) -> f32 {
    newton_interpolation_3(fraction, (0.0, 0.0), (0.7, 1.0), (1.0, 0.0))
}

fn newton_interpolation_3(fraction: f32, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32)) -> f32 {
    let x = fraction.clamp(0.0, 1.0);
    let d0 = p0.1;
    let d1 = (p1.1 - p0.1) / (p1.0 - p0.0);
    let second_left = (p2.1 - p1.1) / (p2.0 - p1.0);
    let d2 = (second_left - d1) / (p2.0 - p0.0);
    d0 + d1 * (x - p0.0) + d2 * (x - p0.0) * (x - p1.0)
}

impl KaraokeBrush {
    fn fade_bounds(self) -> (f32, f32) {
        let total_width = (self.row_max_x - self.row_min_x).max(1.0);
        let fade_range = (FADE_WIDTH / total_width).min(1.0);
        let line_progress = ((self.active_edge - self.row_min_x) / total_width).clamp(0.0, 1.0);
        let fade_center = -fade_range * 0.5 + (1.0 + fade_range) * line_progress;
        let fade_start = self.row_min_x + (fade_center - fade_range * 0.5) * total_width;
        let fade_end = self.row_min_x + (fade_center + fade_range * 0.5) * total_width;
        (fade_start, fade_end)
    }

    fn sample_alpha(self, x: f32) -> f32 {
        let (fade_start, fade_end) = self.fade_bounds();
        let t = if fade_end <= fade_start {
            if x <= fade_start {
                0.0
            } else {
                1.0
            }
        } else {
            ((x - fade_start) / (fade_end - fade_start)).clamp(0.0, 1.0)
        };

        if self.is_rtl {
            self.inactive_alpha + (1.0 - self.inactive_alpha) * t
        } else {
            1.0 - (1.0 - self.inactive_alpha) * t
        }
    }
}

pub(super) fn rgba_from_argb(argb: u32) -> (u8, u8, u8, u8) {
    (
        ((argb >> 16) & 0xff) as u8,
        ((argb >> 8) & 0xff) as u8,
        (argb & 0xff) as u8,
        ((argb >> 24) & 0xff) as u8,
    )
}
