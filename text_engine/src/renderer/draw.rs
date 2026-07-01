use cosmic_text::fontdb;
#[cfg(not(target_os = "android"))]
use cosmic_text::{Color as CosmicColor, FontSystem, PhysicalGlyph, SwashCache};
use skia_safe::{
    canvas::SaveLayerRec,
    font,
    font_arguments::{variation_position::Coordinate, VariationPosition},
    gradient,
    image_filters::{self, CropRect},
    BlurStyle, Color4f, Font, FontArguments, FontHinting, FourByteTag, GlyphId, MaskFilter, Paint,
    Point, Rect, Shader, TileMode, Typeface,
};
use std::collections::HashMap;
use std::f32::consts::PI;

use super::*;

#[cfg(not(target_os = "android"))]
pub(super) fn draw_prepared_text(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    blurred_glyph_cache: &mut HashMap<BlurredGlyphCacheKey, BlurredGlyphMask>,
    text: &PreparedText,
    origin_x: f32,
    origin_y: f32,
    base_color: (u8, u8, u8, u8),
    alpha: f32,
    blur_radius: f32,
    karaoke: Option<(i32, bool, &Vec<PreparedSyllable>)>,
) {
    for row in &text.rows {
        let (row_min_x, row_max_x) =
            row_x_bounds(row, origin_x).unwrap_or((origin_x, origin_x + row.width));
        let active_edge = karaoke.and_then(|(time, is_rtl, syllables)| {
            active_edge_for_row(row, origin_x, time, is_rtl, syllables)
        });
        let karaoke_brush = active_edge.map(|active_edge| KaraokeBrush {
            active_edge,
            row_min_x,
            row_max_x,
            is_rtl: karaoke.map(|(_, is_rtl, _)| is_rtl).unwrap_or(false),
        });

        for (glyph_position, glyph) in row.glyphs.iter().enumerate() {
            let effect = karaoke
                .and_then(|(time, _, syllables)| {
                    glyph_effect_for_time(glyph, row, glyph_position, time, syllables)
                })
                .unwrap_or_default();
            let physical = PhysicalGlyph {
                cache_key: glyph.physical.cache_key,
                x: glyph.physical.x + origin_x.round() as i32,
                y: glyph.physical.y + origin_y.round() as i32 + effect.offset_y.round() as i32,
            };
            let scale_pivot = effect
                .scale_pivot
                .map(|(x, y)| (origin_x + x, origin_y + y));
            let glyph_alpha = alpha * glyph.alpha_multiplier;

            if glyph_alpha <= 0.0 {
                continue;
            }

            if effect.shadow_blur_radius > 0.1 && !glyph.is_phonetic {
                // Keep the shadow locked to the same scale/pivot as the glyph so
                // it tracks the awesome syllable swell instead of lagging behind.
                draw_glyph_with_optional_blur(
                    font_system,
                    swash_cache,
                    pixels,
                    width,
                    height,
                    blurred_glyph_cache,
                    physical.clone(),
                    base_color,
                    glyph_alpha * 0.4,
                    effect.shadow_blur_radius,
                    effect.scale,
                    scale_pivot,
                    karaoke_brush,
                );
            }

            draw_glyph_with_optional_blur(
                font_system,
                swash_cache,
                pixels,
                width,
                height,
                blurred_glyph_cache,
                physical,
                base_color,
                glyph_alpha,
                blur_radius,
                effect.scale,
                scale_pivot,
                karaoke_brush,
            );
        }
    }
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
    karaoke: Option<(i32, bool, &Vec<PreparedSyllable>)>,
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
        if let Some(filter) =
            image_filters::blur((blur_radius, blur_radius), None, None, CropRect::NO_CROP_RECT)
        {
            layer_paint.set_image_filter(filter);
        }
        canvas.save_layer(&SaveLayerRec::default().bounds(&bounds).paint(&layer_paint));
    }
    let inner_blur = if layer_blur { 0.0 } else { blur_radius };

    for row in &text.rows {
        let (row_min_x, row_max_x) =
            row_x_bounds(row, origin_x).unwrap_or((origin_x, origin_x + row.width));
        let active_edge = karaoke.and_then(|(time, is_rtl, syllables)| {
            active_edge_for_row(row, origin_x, time, is_rtl, syllables)
        });
        let karaoke_brush = active_edge.map(|active_edge| KaraokeBrush {
            active_edge,
            row_min_x,
            row_max_x,
            is_rtl: karaoke.map(|(_, is_rtl, _)| is_rtl).unwrap_or(false),
        });
        let karaoke_shader = karaoke_brush.and_then(|brush| make_karaoke_shader(brush, base_color));
        let mut batcher = SkiaGlyphBatcher::default();
        let can_batch_brush = karaoke_shader.is_some() || karaoke_brush.is_none();

        for (glyph_position, glyph) in row.glyphs.iter().enumerate() {
            let effect = karaoke
                .and_then(|(time, _, syllables)| {
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

    with_skia_font(batch.key.font_id, font_size, batch.key.weight, typefaces, |font| {
        canvas.draw_glyphs_at(
            &batch.glyphs,
            &batch.positions[..],
            Point::new(0.0, 0.0),
            font,
            &paint,
        );
    });
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
    with_skia_font(cache_key.font_id, font_size, cache_key.font_weight.0, typefaces, |font| {
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
    });
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
    let arguments = FontArguments::new()
        .set_variation_design_position(VariationPosition { coordinates: &coordinates });
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

fn make_karaoke_shader(brush: KaraokeBrush, base_color: (u8, u8, u8, u8)) -> Option<Shader> {
    let (fade_start, fade_end) = brush.fade_bounds();
    if fade_end <= fade_start {
        return None;
    }

    let active = skia_color(base_color, 1.0);
    let inactive = skia_color(base_color, KARAOKE_INACTIVE_ALPHA);
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

#[cfg(not(target_os = "android"))]
fn draw_glyph_with_optional_blur(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    blurred_glyph_cache: &mut HashMap<BlurredGlyphCacheKey, BlurredGlyphMask>,
    physical: PhysicalGlyph,
    base_color: (u8, u8, u8, u8),
    glyph_alpha: f32,
    blur_radius: f32,
    scale: f32,
    scale_pivot: Option<(f32, f32)>,
    karaoke_brush: Option<KaraokeBrush>,
) {
    if blur_radius <= 0.1 {
        draw_glyph_pixels(
            font_system,
            swash_cache,
            pixels,
            width,
            height,
            physical,
            base_color,
            glyph_alpha,
            scale,
            scale_pivot,
            karaoke_brush,
        );
    } else {
        draw_blurred_glyph_pixels(
            font_system,
            swash_cache,
            pixels,
            width,
            height,
            blurred_glyph_cache,
            physical,
            base_color,
            glyph_alpha,
            blur_radius,
            scale,
            scale_pivot,
            karaoke_brush,
        );
    }
}

#[cfg(not(target_os = "android"))]
fn draw_glyph_pixels(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    physical: PhysicalGlyph,
    base_color: (u8, u8, u8, u8),
    glyph_alpha: f32,
    scale: f32,
    scale_pivot: Option<(f32, f32)>,
    karaoke_brush: Option<KaraokeBrush>,
) {
    let color = CosmicColor::rgba(
        base_color.0,
        base_color.1,
        base_color.2,
        ((base_color.3 as f32) * glyph_alpha)
            .round()
            .clamp(0.0, 255.0) as u8,
    );

    if (scale - 1.0).abs() > 0.01 {
        draw_scaled_glyph_pixels(
            font_system,
            swash_cache,
            pixels,
            width,
            height,
            physical,
            color,
            scale,
            scale_pivot,
            karaoke_brush,
        );
        return;
    }

    swash_cache.with_pixels(font_system, physical.cache_key, color, |x, y, color| {
        let dst_x = physical.x + x;
        let dst_y = physical.y + y;
        let brushed_color = karaoke_brush
            .map(|brush| brush.sample_color(dst_x as f32, color))
            .unwrap_or(color);
        if brushed_color.a() == 0 {
            return;
        }
        blend_pixel(pixels, width, height, dst_x, dst_y, brushed_color);
    });
}

#[cfg(not(target_os = "android"))]
fn draw_scaled_glyph_pixels(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    physical: PhysicalGlyph,
    color: CosmicColor,
    scale: f32,
    scale_pivot: Option<(f32, f32)>,
    karaoke_brush: Option<KaraokeBrush>,
) {
    let mut samples = Vec::new();
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    swash_cache.with_pixels(font_system, physical.cache_key, color, |x, y, color| {
        if color.a() == 0 {
            return;
        }
        let dst_x = physical.x + x;
        let dst_y = physical.y + y;
        min_x = min_x.min(dst_x);
        min_y = min_y.min(dst_y);
        max_x = max_x.max(dst_x);
        max_y = max_y.max(dst_y);
        samples.push((dst_x, dst_y, color));
    });

    if samples.is_empty() {
        return;
    }

    let (center_x, center_y) =
        scale_pivot.unwrap_or_else(|| ((min_x + max_x) as f32 * 0.5, (min_y + max_y) as f32 * 0.5));
    let cover = scale.ceil().max(1.0) as i32;
    let cover_offset = cover / 2;
    for (sample_x, sample_y, color) in samples {
        let scaled_x = (center_x + (sample_x as f32 - center_x) * scale).round() as i32;
        let scaled_y = (center_y + (sample_y as f32 - center_y) * scale).round() as i32;
        for y in 0..cover {
            for x in 0..cover {
                let dst_x = scaled_x + x - cover_offset;
                let dst_y = scaled_y + y - cover_offset;
                let brushed_color = karaoke_brush
                    .map(|brush| brush.sample_color(dst_x as f32, color))
                    .unwrap_or(color);
                blend_pixel(pixels, width, height, dst_x, dst_y, brushed_color);
            }
        }
    }
}

#[cfg(not(target_os = "android"))]
fn draw_blurred_glyph_pixels(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    blurred_glyph_cache: &mut HashMap<BlurredGlyphCacheKey, BlurredGlyphMask>,
    physical: PhysicalGlyph,
    base_color: (u8, u8, u8, u8),
    glyph_alpha: f32,
    blur_radius: f32,
    scale: f32,
    scale_pivot: Option<(f32, f32)>,
    karaoke_brush: Option<KaraokeBrush>,
) {
    let radius = blur_radius.ceil().clamp(1.0, MAX_GLYPH_BLUR_RADIUS) as u8;
    let Some(mask) = get_blurred_glyph_mask(
        font_system,
        swash_cache,
        blurred_glyph_cache,
        physical.cache_key,
        radius,
    ) else {
        return;
    };

    let color_alpha = (base_color.3 as f32) * glyph_alpha.clamp(0.0, 1.0);
    if color_alpha <= 0.0 {
        return;
    }

    let scaled = (scale - 1.0).abs() > 0.01;
    let (pivot_x, pivot_y) = scale_pivot.unwrap_or_else(|| {
        (
            (physical.x + mask.origin_x) as f32 + mask.width as f32 * 0.5,
            (physical.y + mask.origin_y) as f32 + mask.height as f32 * 0.5,
        )
    });
    let cover = if scaled {
        scale.ceil().max(1.0) as i32
    } else {
        1
    };
    let cover_offset = cover / 2;

    for local_y in 0..mask.height {
        let base_y = physical.y + mask.origin_y + local_y as i32;
        for local_x in 0..mask.width {
            let alpha = mask.alpha[local_y * mask.width + local_x];
            if alpha == 0 {
                continue;
            }
            let base_x = physical.x + mask.origin_x + local_x as i32;
            let color = CosmicColor::rgba(
                base_color.0,
                base_color.1,
                base_color.2,
                ((alpha as f32 / 255.0) * color_alpha)
                    .round()
                    .clamp(0.0, 255.0) as u8,
            );
            if scaled {
                let scaled_x = (pivot_x + (base_x as f32 - pivot_x) * scale).round() as i32;
                let scaled_y = (pivot_y + (base_y as f32 - pivot_y) * scale).round() as i32;
                for y in 0..cover {
                    for x in 0..cover {
                        let dst_x = scaled_x + x - cover_offset;
                        let dst_y = scaled_y + y - cover_offset;
                        let brushed_color = karaoke_brush
                            .map(|brush| brush.sample_color(dst_x as f32, color))
                            .unwrap_or(color);
                        blend_pixel(pixels, width, height, dst_x, dst_y, brushed_color);
                    }
                }
            } else {
                let brushed_color = karaoke_brush
                    .map(|brush| brush.sample_color(base_x as f32, color))
                    .unwrap_or(color);
                blend_pixel(pixels, width, height, base_x, base_y, brushed_color);
            }
        }
    }
}

#[cfg(not(target_os = "android"))]
fn get_blurred_glyph_mask<'a>(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    blurred_glyph_cache: &'a mut HashMap<BlurredGlyphCacheKey, BlurredGlyphMask>,
    cache_key: cosmic_text::CacheKey,
    radius: u8,
) -> Option<&'a BlurredGlyphMask> {
    let key = BlurredGlyphCacheKey { cache_key, radius };
    if !blurred_glyph_cache.contains_key(&key) {
        let mask = build_blurred_glyph_mask(font_system, swash_cache, cache_key, radius)?;
        blurred_glyph_cache.insert(key, mask);
    }
    blurred_glyph_cache.get(&key)
}

#[cfg(not(target_os = "android"))]
fn build_blurred_glyph_mask(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    cache_key: cosmic_text::CacheKey,
    radius: u8,
) -> Option<BlurredGlyphMask> {
    let color = CosmicColor::rgba(255, 255, 255, 255);
    let mut samples = Vec::new();
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    swash_cache.with_pixels(font_system, cache_key, color, |x, y, color| {
        if color.a() == 0 {
            return;
        }
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        samples.push((x, y, color.a() as f32 / 255.0));
    });

    if samples.is_empty() {
        return None;
    }

    let radius = radius as i32;
    let pad = radius;
    let origin_x = min_x - pad;
    let origin_y = min_y - pad;
    let mask_width = (max_x - min_x + 1 + pad * 2).max(1) as usize;
    let mask_height = (max_y - min_y + 1 + pad * 2).max(1) as usize;
    let mut mask = vec![0.0f32; mask_width * mask_height];

    for (x, y, alpha) in samples {
        let local_x = (x - origin_x) as usize;
        let local_y = (y - origin_y) as usize;
        let index = local_y * mask_width + local_x;
        mask[index] = mask[index].max(alpha);
    }

    let blurred = box_blur_alpha(&mask, mask_width, mask_height, radius as usize);
    let alpha = blurred
        .into_iter()
        .map(|value| {
            if value <= 1.0 / 255.0 {
                0
            } else {
                (value * 255.0).round().clamp(0.0, 255.0) as u8
            }
        })
        .collect::<Vec<_>>();

    Some(BlurredGlyphMask {
        origin_x,
        origin_y,
        width: mask_width,
        height: mask_height,
        alpha,
    })
}

#[cfg(not(target_os = "android"))]
fn box_blur_alpha(source: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    if source.is_empty() || width == 0 || height == 0 || radius == 0 {
        return source.to_vec();
    }

    let mut horizontal = vec![0.0f32; source.len()];
    let mut output = vec![0.0f32; source.len()];
    let kernel = (radius * 2 + 1) as f32;

    for y in 0..height {
        let row_start = y * width;
        let row = &source[row_start..row_start + width];
        let mut prefix = vec![0.0f32; width + 1];
        for x in 0..width {
            prefix[x + 1] = prefix[x] + row[x];
        }
        for x in 0..width {
            let start = x.saturating_sub(radius);
            let end = (x + radius + 1).min(width);
            horizontal[row_start + x] = (prefix[end] - prefix[start]) / kernel;
        }
    }

    let mut prefix = vec![0.0f32; height + 1];
    for x in 0..width {
        prefix.fill(0.0);
        for y in 0..height {
            prefix[y + 1] = prefix[y] + horizontal[y * width + x];
        }
        for y in 0..height {
            let start = y.saturating_sub(radius);
            let end = (y + radius + 1).min(height);
            output[y * width + x] = (prefix[end] - prefix[start]) / kernel;
        }
    }

    output
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

#[cfg(not(target_os = "android"))]
pub(super) fn draw_breathing_dots(
    pixels: &mut [u8],
    width: u32,
    height: u32,
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
        config.width as f32 - config.padding_x - total_width
    } else {
        config.padding_x
    };
    let origin_y = y + config.padding_y;
    let (scale, alpha, reveal_progress) =
        breathing_dots_state(interlude.start, interlude.end, current_time_ms, dots);
    if alpha <= 0.0 || scale <= 0.0 {
        return;
    }

    let color = rgba_from_argb(dots.color);
    let current_time = current_time_ms as f32;
    let center_x = origin_x + total_width * 0.5;
    let center_y = origin_y + dots.size * 0.5;
    let reveal_x = origin_x + total_width * reveal_progress.clamp(0.0, 1.0);
    let dot_duration =
        ((interlude.end - interlude.start) as f32 - dots.enter_ms - dots.exit_ms - dots.still_ms)
            .max(1.0)
            / dots.number as f32;

    for index in 0..dots.number {
        let base_x = origin_x + dots.size * 0.5 + (dots.size + dots.margin) * index as f32;
        let base_y = origin_y + dots.size * 0.5;
        let scaled_x = center_x + (base_x - center_x) * scale;
        let scaled_y = center_y + (base_y - center_y) * scale;
        let radius = dots.size * 0.5 * scale;
        let reveal_alpha = ((reveal_x - base_x + dots.size * 0.5) / dots.size).clamp(0.0, 1.0);
        let dot_start = interlude.start as f32 + dots.enter_ms + dot_duration * index as f32;
        let dot_alpha = if current_time >= interlude.start as f32 + dots.enter_ms {
            ((current_time - dot_start) / dot_duration).clamp(0.0, 1.0) * 0.6 + 0.4
        } else {
            0.4
        };

        draw_circle_rgba(
            pixels,
            width,
            height,
            scaled_x,
            scaled_y,
            radius,
            color,
            alpha * dot_alpha * reveal_alpha * line_alpha,
        );
    }
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
        config.width as f32 - config.padding_x - total_width
    } else {
        config.padding_x
    };
    let origin_y = y + config.padding_y;
    let (scale, alpha, reveal_progress) =
        breathing_dots_state(interlude.start, interlude.end, current_time_ms, dots);
    if alpha <= 0.0 || scale <= 0.0 {
        return;
    }

    let color = rgba_from_argb(dots.color);
    let current_time = current_time_ms as f32;
    let center_x = origin_x + total_width * 0.5;
    let center_y = origin_y + dots.size * 0.5;
    let reveal_x = origin_x + total_width * reveal_progress.clamp(0.0, 1.0);
    let dot_duration =
        ((interlude.end - interlude.start) as f32 - dots.enter_ms - dots.exit_ms - dots.still_ms)
            .max(1.0)
            / dots.number as f32;

    for index in 0..dots.number {
        let base_x = origin_x + dots.size * 0.5 + (dots.size + dots.margin) * index as f32;
        let base_y = origin_y + dots.size * 0.5;
        let scaled_x = center_x + (base_x - center_x) * scale;
        let scaled_y = center_y + (base_y - center_y) * scale;
        let radius = dots.size * 0.5 * scale;
        let reveal_alpha = ((reveal_x - base_x + dots.size * 0.5) / dots.size).clamp(0.0, 1.0);
        let dot_start = interlude.start as f32 + dots.enter_ms + dot_duration * index as f32;
        let dot_alpha = if current_time >= interlude.start as f32 + dots.enter_ms {
            ((current_time - dot_start) / dot_duration).clamp(0.0, 1.0) * 0.6 + 0.4
        } else {
            0.4
        };

        let total_alpha = alpha * dot_alpha * reveal_alpha * line_alpha;
        if radius <= 0.0 || total_alpha <= 0.0 {
            continue;
        }

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(skia_color(color, total_alpha), None);
        canvas.draw_circle(Point::new(scaled_x, scaled_y), radius, &paint);
    }
}

fn breathing_dots_state(
    start_ms: i32,
    end_ms: i32,
    current_time_ms: i32,
    dots: BreathingDotsConfig,
) -> (f32, f32, f32) {
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

    if current < enter_end {
        let progress = ((current - start) / (enter_end - start).max(1.0)).clamp(0.0, 1.0);
        let eased = smooth_step(progress);
        return (eased * 0.8, eased, eased);
    }
    if current < dip_start {
        // Breathe at ~3000ms/cycle, but stretch the period slightly so a whole
        // number of HALF-cycles fits the (variable-length) breathing window and
        // it always ends at a peak (value 1.0) — exactly where the dip phase
        // begins. With the old fixed 3000ms period the window ended at an
        // arbitrary phase, leaving a leftover ~half oscillation that read as an
        // "extra half cycle" before the dip. `0.9 - 0.1·cos`: starts at 0.8
        // (matching the enter end), peaks at 1.0 every odd half-cycle.
        let breathing_duration = (dip_start - enter_end).max(1.0);
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
        return (0.9 - 0.1 * angle.cos(), 1.0, 1.0);
    }
    if current < still_start {
        let progress = ((current - dip_start) / (still_start - dip_start).max(1.0)).clamp(0.0, 1.0);
        return (0.8 + 0.2 * (progress * 2.0 * PI).cos(), 1.0, 1.0);
    }
    if current < exit_start {
        return (1.0, 1.0, 1.0);
    }

    let progress = ((end - current) / (end - exit_start).max(1.0)).clamp(0.0, 1.0);
    let eased = smooth_step(progress);
    (eased, eased, 1.0)
}

fn dots_total_width(dots: BreathingDotsConfig) -> f32 {
    dots.size * dots.number as f32 + dots.margin * dots.number.saturating_sub(1) as f32
}

#[cfg(not(target_os = "android"))]
fn draw_circle_rgba(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    radius: f32,
    color: (u8, u8, u8, u8),
    alpha: f32,
) {
    if radius <= 0.0 || alpha <= 0.0 {
        return;
    }

    let min_x = (cx - radius - 1.0).floor() as i32;
    let max_x = (cx + radius + 1.0).ceil() as i32;
    let min_y = (cy - radius - 1.0).floor() as i32;
    let max_y = (cy + radius + 1.0).ceil() as i32;
    let color_alpha = color.3 as f32 * alpha.clamp(0.0, 1.0);

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let distance = (dx * dx + dy * dy).sqrt();
            let edge_alpha = (radius + 0.5 - distance).clamp(0.0, 1.0);
            if edge_alpha <= 0.0 {
                continue;
            }
            blend_pixel(
                pixels,
                width,
                height,
                x,
                y,
                CosmicColor::rgba(
                    color.0,
                    color.1,
                    color.2,
                    (color_alpha * edge_alpha).round().clamp(0.0, 255.0) as u8,
                ),
            );
        }
    }
}

#[cfg(not(target_os = "android"))]
pub(super) fn apply_vertical_fade(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    top_px: f32,
    bottom_px: f32,
) {
    if width == 0 || height == 0 {
        return;
    }

    for y in 0..height {
        let top_factor = if top_px > 0.0 {
            (y as f32 / top_px).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let bottom_distance = height.saturating_sub(1).saturating_sub(y) as f32;
        let bottom_factor = if bottom_px > 0.0 {
            (bottom_distance / bottom_px).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let factor = top_factor.min(bottom_factor);
        if factor >= 0.999 {
            continue;
        }
        let row_start = (y * width * 4) as usize;
        let row_end = (row_start + width as usize * 4).min(pixels.len());
        let row = &mut pixels[row_start..row_end];
        for pixel in row.chunks_exact_mut(4) {
            pixel[3] = ((pixel[3] as f32) * factor).round().clamp(0.0, 255.0) as u8;
        }
    }
}

fn smooth_step(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(super) fn accompaniment_visibility(start_ms: i32, end_ms: i32, current_time_ms: i32) -> f32 {
    // Expand the accompaniment line into place shortly before it starts, then ease
    // it back out after it ends. The grow/shrink is a deterministic eased height.
    // Kept short and roughly matched to the scroll spring's settle time (~0.5s) so
    // the make-room animation harmonizes with the auto-scroll instead of dragging
    // on for a second-plus — a long animation overlaps the next line's expand and
    // makes the focus bob (the "trembling" when two consecutive lines both have an
    // accompaniment).
    const ENTER_MS: f32 = 400.0;
    const EXIT_LINGER_MS: f32 = 200.0;
    const EXIT_FADE_MS: f32 = 400.0;

    let start = start_ms as f32;
    let end = end_ms as f32;
    let current = current_time_ms as f32;
    let enter_start = start - ENTER_MS;
    let exit_start = end + EXIT_LINGER_MS;
    let exit_end = exit_start + EXIT_FADE_MS;

    if current < enter_start || current > exit_end {
        0.0
    } else if current < start {
        smooth_step((current - enter_start) / ENTER_MS)
    } else if current <= exit_start {
        1.0
    } else {
        smooth_step((exit_end - current) / EXIT_FADE_MS)
    }
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
    #[cfg(not(target_os = "android"))]
    fn sample_color(self, x: f32, glyph_color: CosmicColor) -> CosmicColor {
        let alpha = self.sample_alpha(x);
        CosmicColor::rgba(
            glyph_color.r(),
            glyph_color.g(),
            glyph_color.b(),
            ((glyph_color.a() as f32) * alpha).round().clamp(0.0, 255.0) as u8,
        )
    }

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
            KARAOKE_INACTIVE_ALPHA + (1.0 - KARAOKE_INACTIVE_ALPHA) * t
        } else {
            1.0 - (1.0 - KARAOKE_INACTIVE_ALPHA) * t
        }
    }
}

#[cfg(not(target_os = "android"))]
fn blend_pixel(pixels: &mut [u8], width: u32, height: u32, x: i32, y: i32, color: CosmicColor) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return;
    }

    let index = ((y as u32 * width + x as u32) * 4) as usize;
    if index + 3 >= pixels.len() {
        return;
    }

    let source_a = color.a() as f32 / 255.0;
    if source_a <= 0.0 {
        return;
    }

    let source_r = color.r() as f32;
    let source_g = color.g() as f32;
    let source_b = color.b() as f32;
    let dest_a = pixels[index + 3] as f32 / 255.0;
    let out_a = source_a + dest_a * (1.0 - source_a);
    if out_a <= 0.0 {
        pixels[index] = 0;
        pixels[index + 1] = 0;
        pixels[index + 2] = 0;
        pixels[index + 3] = 0;
        return;
    }

    pixels[index] = ((source_r * source_a + pixels[index] as f32 * dest_a * (1.0 - source_a))
        / out_a)
        .round()
        .clamp(0.0, 255.0) as u8;
    pixels[index + 1] =
        ((source_g * source_a + pixels[index + 1] as f32 * dest_a * (1.0 - source_a)) / out_a)
            .round()
            .clamp(0.0, 255.0) as u8;
    pixels[index + 2] =
        ((source_b * source_a + pixels[index + 2] as f32 * dest_a * (1.0 - source_a)) / out_a)
            .round()
            .clamp(0.0, 255.0) as u8;
    pixels[index + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

pub(super) fn rgba_from_argb(argb: u32) -> (u8, u8, u8, u8) {
    (
        ((argb >> 16) & 0xff) as u8,
        ((argb >> 8) & 0xff) as u8,
        (argb & 0xff) as u8,
        ((argb >> 24) & 0xff) as u8,
    )
}
