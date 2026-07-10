use memmap2::Mmap;
use sdf_glyph_renderer::{clamp_to_u8, BitmapGlyph};
use std::ops::Deref;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::Format;
use swash::FontRef;

/// Buffer size around the glyph for SDF spread (increased for shadow support)
const SDF_BUFFER: usize = 16;
/// SDF radius (distance field cutoff) - increased to match buffer for full shadow range
const SDF_RADIUS: usize = 16;
/// SDF cutoff for clamping to u8 (0.25 is standard for text rendering)
const SDF_CUTOFF: f64 = 0.25;

/// Font data storage - supports both owned bytes and memory-mapped files
pub enum FontData {
    Owned(Vec<u8>),
    Mapped {
        mmap: Mmap,
        start: usize,
        len: usize,
    },
}

impl Deref for FontData {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            FontData::Owned(v) => v.as_slice(),
            FontData::Mapped { mmap, start, len } => {
                let end = start.saturating_add(*len).min(mmap.len());
                &mmap.deref()[*start..end]
            }
        }
    }
}

pub struct FontWrapper {
    pub font_data: FontData,
    pub _id: usize,
    pub face_index: u32,
    scale_context: ScaleContext,
}

impl FontWrapper {
    pub fn from_bytes_with_index(bytes: &[u8], id: usize, face_index: u32) -> Option<Self> {
        // Verify font is valid
        let _ = FontRef::from_index(bytes, face_index as usize)?;
        Some(Self {
            font_data: FontData::Owned(bytes.to_vec()),
            _id: id,
            face_index,
            scale_context: ScaleContext::new(),
        })
    }

    pub fn from_mmap_with_index(mmap: Mmap, id: usize, face_index: u32) -> Option<Self> {
        Self::from_mmap_with_range(mmap, id, face_index, 0, None)
    }

    pub fn from_mmap_with_range(
        mmap: Mmap,
        id: usize,
        face_index: u32,
        start: usize,
        len: Option<usize>,
    ) -> Option<Self> {
        let available_len = mmap.len().checked_sub(start)?;
        let len = len.unwrap_or(available_len).min(available_len);
        let data = &mmap[start..start + len];

        // Verify font is valid
        let _ = FontRef::from_index(data, face_index as usize)?;
        Some(Self {
            font_data: FontData::Mapped { mmap, start, len },
            _id: id,
            face_index,
            scale_context: ScaleContext::new(),
        })
    }

    /// Generate a Signed Distance Field for the given glyph with variable font weight support.
    /// Returns (rgba_data, width, height, xmin, ymin) where the SDF is stored in the alpha channel.
    /// RGB channels are set to 255 (white) for shader flexibility.
    /// xmin, ymin are the bearing offsets from glyph origin to bitmap top-left.
    ///
    /// weight: Font weight (100-900, where 400=normal, 700=bold)
    pub fn generate_sdf(
        &mut self,
        glyph_id: u16,
        size_px: f32,
        weight: f32,
    ) -> (Vec<u8>, u32, u32, f32, f32) {
        // Create FontRef directly to avoid borrow conflicts
        let font = match FontRef::from_index(&self.font_data, self.face_index as usize) {
            Some(f) => f,
            None => return (vec![0, 0, 0, 0], 1, 1, 0.0, 0.0),
        };

        // Build scaler with variable font weight support
        let mut scaler = self
            .scale_context
            .builder(font)
            .size(size_px)
            .hint(true)
            .variations(&[("wght", weight)]) // Set font weight axis
            .build();

        // Render the glyph to an alpha mask
        let image = Render::new(&[Source::Outline])
            .format(Format::Alpha)
            .render(&mut scaler, glyph_id);

        let image = match image {
            Some(img) => img,
            None => return (vec![0, 0, 0, 0], 1, 1, 0.0, 0.0),
        };

        let width = image.placement.width as usize;
        let height = image.placement.height as usize;

        if width == 0 || height == 0 {
            // Empty glyph (e.g., space character)
            return (vec![0, 0, 0, 0], 1, 1, 0.0, 0.0);
        }

        // Use from_unbuffered which automatically adds padding for SDF spread
        let glyph_bitmap =
            match BitmapGlyph::from_unbuffered(&image.data, width, height, SDF_BUFFER) {
                Ok(bmp) => bmp,
                Err(_) => {
                    // Fallback: return a 1x1 transparent pixel
                    return (vec![0, 0, 0, 0], 1, 1, 0.0, 0.0);
                }
            };

        // Generate the signed distance field
        let sdf_f64 = glyph_bitmap.render_sdf(SDF_RADIUS);

        // Convert f64 SDF values to u8
        let sdf_u8 = match clamp_to_u8(&sdf_f64, SDF_CUTOFF) {
            Ok(data) => data,
            Err(_) => {
                // Fallback: return a 1x1 transparent pixel
                return (vec![0, 0, 0, 0], 1, 1, 0.0, 0.0);
            }
        };

        // Calculate output dimensions (original + buffer on each side)
        let output_width = width + (SDF_BUFFER * 2);
        let output_height = height + (SDF_BUFFER * 2);

        // SDF processing parameters (matching Kotlin values)
        const SDF_THRESHOLD: f32 = 0.7;
        const SDF_SMOOTHING: f32 = 0.02;
        const SHADOW_OUTER_EDGE: f32 = 0.4;
        const SHADOW_INNER_EDGE: f32 = SDF_THRESHOLD;

        // Convert single-channel SDF to RGBA format with processing
        // Normal: smoothstep around threshold
        // Shadow: smoothstep falloff for glow
        let mut rgba_data = Vec::with_capacity(sdf_u8.len() * 4);
        for &sdf_byte in &sdf_u8 {
            let sdf_value = sdf_byte as f32 / 255.0;

            // Normal text: smoothstep around threshold
            let normal_alpha = smoothstep(
                SDF_THRESHOLD - SDF_SMOOTHING,
                SDF_THRESHOLD + SDF_SMOOTHING,
                sdf_value,
            );

            // Shadow: smoothstep falloff
            let shadow_alpha = if sdf_value >= SHADOW_INNER_EDGE {
                0.0 // Inside text - covered by text layer
            } else if sdf_value <= SHADOW_OUTER_EDGE {
                0.0 // At buffer edge
            } else {
                let t = (sdf_value - SHADOW_OUTER_EDGE) / (SHADOW_INNER_EDGE - SHADOW_OUTER_EDGE);
                t * t * (3.0 - 2.0 * t) // smoothstep
            };

            // Pack both alphas: normal in R channel, shadow in G channel
            // RGB will be set to 255, actual color applied at draw time
            // Using A channel for normal alpha (backward compatible)
            rgba_data.push(255); // R - white
            rgba_data.push((shadow_alpha * 255.0) as u8); // G - shadow alpha
            rgba_data.push(255); // B - white
            rgba_data.push((normal_alpha * 255.0) as u8); // A - normal alpha
        }

        // Bearing offsets from swash placement (already in pixels)
        // Adjust for SDF buffer padding
        let xmin = image.placement.left as f32 - SDF_BUFFER as f32;
        let ymin = (image.placement.top as i32 - height as i32) as f32 - SDF_BUFFER as f32;

        (
            rgba_data,
            output_width as u32,
            output_height as u32,
            xmin,
            ymin,
        )
    }
}

/// Smoothstep function for smooth alpha transitions
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
