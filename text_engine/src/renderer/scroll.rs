//! Scroll & spring animation: manual (touch) scrolling with fling and
//! rubber-banding, the depth-of-field blur release, the per-line scroll-spring
//! cascade, and the seek/auto-scroll animation. Split out of `renderer.rs`.

use super::*;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub(super) struct SpringLineState {
    scroll: f32,
    velocity: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ManualScrollState {
    offset: f32,
    velocity: f32,
    dragging: bool,
    hold_until: Option<Instant>,
    /// Blur stays released until this instant. Set purely from real touch input
    /// (grab / drag / release / cancel) and never touched by the fling/return
    /// physics, so the automatic glide-back can't re-trigger the blur.
    blur_engaged_until: Option<Instant>,
}


impl LyricsRenderer {
    pub(super) fn reset_layout_animation_state(&mut self) {
        self.spring_layouts.clear();
        self.last_spring_frame_at = None;
        self.last_spring_playback_ms = None;
        self.last_target_scroll_y = None;
        self.layout_animation_active = false;
        self.seek_glide_active = false;
    }

    pub fn reset_manual_scroll(&mut self) {
        self.manual_scroll = ManualScrollState::default();
        self.last_manual_scroll_frame_at = None;
        self.manual_scroll_active = false;
        self.manual_scroll_blur_release = 0.0;
    }

    fn engage_manual_scroll_blur(&mut self, now: Instant) {
        self.manual_scroll.blur_engaged_until =
            Some(now + Duration::from_millis(MANUAL_SCROLL_BLUR_RESTORE_MS));
    }

    pub fn begin_manual_scroll(&mut self) {
        let now = Instant::now();
        self.manual_scroll.dragging = true;
        self.manual_scroll.velocity = 0.0;
        self.manual_scroll.hold_until = None;
        self.manual_scroll_active = true;
        self.last_manual_scroll_frame_at = Some(now);
        self.engage_manual_scroll_blur(now);
    }

    pub fn scroll_manual_by(&mut self, delta_y: f32) {
        if !delta_y.is_finite() {
            return;
        }
        self.manual_scroll.offset += delta_y;
        self.manual_scroll.velocity = 0.0;
        self.manual_scroll.hold_until = None;
        self.manual_scroll.dragging = true;
        self.manual_scroll_active = true;
        self.engage_manual_scroll_blur(Instant::now());
    }

    pub fn end_manual_scroll(&mut self, velocity_y: f32) {
        let now = Instant::now();
        self.manual_scroll.dragging = false;
        self.manual_scroll.velocity = velocity_y.clamp(
            -MANUAL_SCROLL_MAX_FLING_VELOCITY,
            MANUAL_SCROLL_MAX_FLING_VELOCITY,
        );
        if self.manual_scroll.velocity.abs() <= MANUAL_SCROLL_VELOCITY_EPSILON {
            self.manual_scroll.velocity = 0.0;
            self.manual_scroll.hold_until =
                Some(now + Duration::from_millis(MANUAL_SCROLL_HOLD_MS));
        } else {
            self.manual_scroll.hold_until = None;
        }
        self.manual_scroll_active = true;
        self.last_manual_scroll_frame_at = Some(now);
        self.engage_manual_scroll_blur(now);
    }

    pub fn cancel_manual_scroll(&mut self) {
        let now = Instant::now();
        self.manual_scroll.dragging = false;
        self.manual_scroll.velocity = 0.0;
        self.manual_scroll.hold_until =
            Some(now + Duration::from_millis(MANUAL_SCROLL_HOLD_MS / 2));
        self.manual_scroll_active = self.manual_scroll.offset.abs() > LINE_LAYOUT_EPSILON;
        self.engage_manual_scroll_blur(now);
    }

    pub(super) fn update_manual_scroll_target(&mut self, auto_scroll_y: f32, max_scroll_y: f32) -> f32 {
        let now = Instant::now();
        let dt = self
            .last_manual_scroll_frame_at
            .map(|last| now.duration_since(last).as_secs_f32())
            .unwrap_or(0.0)
            .clamp(0.001, LINE_LAYOUT_MAX_DT);
        self.last_manual_scroll_frame_at = Some(now);

        let mut active = self.manual_scroll.dragging;
        if !self.manual_scroll.dragging {
            if self.manual_scroll.velocity.abs() > MANUAL_SCROLL_VELOCITY_EPSILON {
                self.manual_scroll.offset += self.manual_scroll.velocity * dt;
                self.manual_scroll.velocity *= (-MANUAL_SCROLL_FLING_FRICTION * dt).exp();
                if self.manual_scroll.velocity.abs() <= MANUAL_SCROLL_VELOCITY_EPSILON {
                    self.manual_scroll.velocity = 0.0;
                    self.manual_scroll.hold_until =
                        Some(now + Duration::from_millis(MANUAL_SCROLL_HOLD_MS));
                }
                active = true;
            }

            let lower_offset = -auto_scroll_y;
            let upper_offset = max_scroll_y - auto_scroll_y;
            let bounded_offset = self.manual_scroll.offset.clamp(lower_offset, upper_offset);
            let overscrolled =
                (self.manual_scroll.offset - bounded_offset).abs() > LINE_LAYOUT_EPSILON;
            if overscrolled {
                self.manual_scroll.hold_until = None;
                active |= spring_step(
                    &mut self.manual_scroll.offset,
                    &mut self.manual_scroll.velocity,
                    bounded_offset,
                    MANUAL_SCROLL_OVERSCROLL_STIFFNESS,
                    MANUAL_SCROLL_OVERSCROLL_DAMPING,
                    dt,
                );
            } else if self.manual_scroll.velocity == 0.0
                && self.manual_scroll.offset.abs() > LINE_LAYOUT_EPSILON
            {
                let hold_until = self
                    .manual_scroll
                    .hold_until
                    .get_or_insert_with(|| now + Duration::from_millis(MANUAL_SCROLL_HOLD_MS));
                if now >= *hold_until {
                    active |= spring_step(
                        &mut self.manual_scroll.offset,
                        &mut self.manual_scroll.velocity,
                        0.0,
                        MANUAL_SCROLL_RETURN_STIFFNESS,
                        MANUAL_SCROLL_RETURN_DAMPING,
                        dt,
                    );
                } else {
                    active = true;
                }
            } else if self.manual_scroll.offset.abs() <= LINE_LAYOUT_EPSILON
                && self.manual_scroll.velocity.abs() <= MANUAL_SCROLL_VELOCITY_EPSILON
            {
                self.manual_scroll.offset = 0.0;
                self.manual_scroll.velocity = 0.0;
                self.manual_scroll.hold_until = None;
            }
        }

        let mut manual_scroll_active = active
            || self.manual_scroll.dragging
            || self.manual_scroll.offset.abs() > LINE_LAYOUT_EPSILON
            || self.manual_scroll.velocity.abs() > MANUAL_SCROLL_VELOCITY_EPSILON;

        // Depth-of-field blur is released while the finger is down and for a
        // fixed window after the last touch input, then eases back in. This is a
        // pure, monotonic timer driven only by touch events — the fling/return
        // physics never touch `blur_engaged_until`, so the automatic glide-back
        // to the active line (or normal playback auto-scroll) can never flip the
        // blur off/on again. Keep rendering while the blur is engaged or fading
        // so the ease-in still runs even when playback is paused.
        let blur_engaged = self.manual_scroll.dragging
            || self
                .manual_scroll
                .blur_engaged_until
                .is_some_and(|until| now < until);
        let blur_target = if blur_engaged { 1.0 } else { 0.0 };
        if blur_engaged {
            manual_scroll_active = true;
        }
        if (self.manual_scroll_blur_release - blur_target).abs() > 0.001 {
            let rate = if blur_target > self.manual_scroll_blur_release {
                MANUAL_SCROLL_BLUR_FADE_OUT_RATE
            } else {
                MANUAL_SCROLL_BLUR_FADE_IN_RATE
            };
            let factor = 1.0 - (-rate * dt).exp();
            self.manual_scroll_blur_release += (blur_target - self.manual_scroll_blur_release) * factor;
            if (self.manual_scroll_blur_release - blur_target).abs() <= 0.001 {
                self.manual_scroll_blur_release = blur_target;
            } else {
                manual_scroll_active = true;
            }
        }

        self.manual_scroll_active = manual_scroll_active;
        self.manual_scroll_projected_scroll(auto_scroll_y, max_scroll_y)
    }

    pub(super) fn manual_scroll_projected_scroll(&self, auto_scroll_y: f32, max_scroll_y: f32) -> f32 {
        let raw_scroll_y = auto_scroll_y + self.manual_scroll.offset;
        rubber_band_scroll(raw_scroll_y, max_scroll_y)
    }

    /// Advances the per-line scroll springs one frame and returns each line's
    /// on-screen layout. The content-space top and height come straight from
    /// `target_layouts` (deterministic, already eased), and only the *scroll*
    /// offset is sprung per line — so a row's screen top is
    /// `content_top - scroll[i]`.
    ///
    /// Splitting scroll out from the layout is what stops interlude/accompaniment
    /// resizes from vibrating: a height change moves the deterministic content
    /// tops smoothly without perturbing any spring. Meanwhile the focused row's
    /// scroll spring is stiff and far rows soften/lag and couple to neighbours,
    /// so a focus change still ripples through the list like a spring chain.
    /// Advances the per-line scroll springs one frame and writes the projected,
    /// on-screen layouts into the reused `frame_layouts` buffer. The caller reads
    /// the result back via `&self.frame_layouts` — kept out of the return type so
    /// the (exclusive) `&mut self` borrow ends here and the draw pass can share
    /// `self` again.
    pub(super) fn animate_frame_layout(
        &mut self,
        current_time_ms: i32,
        target_layouts: &[DynamicLineLayout],
        target_scroll_y: f32,
        viewport_height: f32,
        focus_end: usize,
    ) {
        let now = Instant::now();

        // Snap (rather than animate) only when the geometry can't be carried
        // over: the scene changed, this is the first frame, or the scroll has to
        // jump further than a tap could ever require. A tap-to-seek lands on a
        // visible row, so its scroll delta stays under the threshold and springs
        // the list to the new position — giving the seek its scroll animation.
        let seek_reset_distance =
            (viewport_height * LINE_LAYOUT_SEEK_RESET_DISTANCE_FACTOR).max(1.0);
        let scroll_jump = self
            .last_target_scroll_y
            .map(|last| (target_scroll_y - last).abs());
        // Don't snap while a manual scroll/fling/return is in flight — that
        // motion is user-driven and always smooth, and a large spring-back could
        // otherwise trip the distance threshold and make the list jump.
        let should_reset = target_layouts.len() != self.spring_layouts.len()
            || self.last_spring_playback_ms.is_none()
            || (!self.manual_scroll_active
                && scroll_jump.is_none_or(|jump| jump > seek_reset_distance));

        if should_reset {
            self.spring_layouts = vec![
                SpringLineState {
                    scroll: target_scroll_y,
                    velocity: 0.0,
                };
                target_layouts.len()
            ];
            self.last_spring_frame_at = Some(now);
            self.last_spring_playback_ms = Some(current_time_ms);
            self.last_target_scroll_y = Some(target_scroll_y);
            self.layout_animation_active = false;
            self.seek_glide_active = false;
            self.project_uniform(target_layouts, target_scroll_y);
            return;
        }

        let dt = self
            .last_spring_frame_at
            .map(|last| now.duration_since(last).as_secs_f32())
            .unwrap_or(0.0)
            .clamp(0.001, LINE_LAYOUT_MAX_DT);
        self.last_spring_frame_at = Some(now);
        // A discontinuous playback-time jump means a seek (the user tapped a
        // lyric), not playback advancing frame-by-frame. Latch a glide so the
        // cascade stays suspended until the list settles at the new position —
        // it resumes for natural progression once the springs reach the target.
        let seek_jump = self.last_spring_playback_ms.is_some_and(|last| {
            let delta = current_time_ms - last;
            delta < -LINE_LAYOUT_SEEK_BACKWARD_MS || delta > LINE_LAYOUT_SEEK_FORWARD_MS
        });
        if seek_jump {
            self.seek_glide_active = true;
        }
        self.last_spring_playback_ms = Some(current_time_ms);
        self.last_target_scroll_y = Some(target_scroll_y);

        // While the finger is down the list must track 1:1, so snap every line's
        // scroll to the target and skip the cascade (which is meant for
        // auto-scroll and fling, not direct dragging).
        if self.manual_scroll.dragging {
            for state in self.spring_layouts.iter_mut() {
                state.scroll = target_scroll_y;
                state.velocity = 0.0;
            }
            self.layout_animation_active = false;
            self.project_uniform(target_layouts, target_scroll_y);
            return;
        }

        // Everything from the focused row upward moves as one rigid block: those
        // rows share the full-stiffness spring and take no coupling, so they just
        // shove up together to clear room for the focused row. The spring chain —
        // softening/lagging with distance and coupled to the row above — only runs
        // *below* the focus, so the upcoming lines stretch and settle one after
        // another while the already-sung lines above leave cleanly as a slab.
        let count = self.spring_layouts.len();
        // While a seek glides, every row chases the single target as one rigid
        // block (no chain, full response) so the list slides smoothly to the new
        // position. The cascade — which seeds each row's target from the row
        // above — only runs for natural playback, where the lag it reads is the
        // small spring lag rather than the stale gap left by a focus-index jump.
        let seek_glide = self.seek_glide_active;
        let anchor_hi = focus_end.min(count.saturating_sub(1));
        self.spring_chained_targets.clear();
        self.spring_chained_targets.resize(count, target_scroll_y);
        if !seek_glide {
            for index in (anchor_hi + 1)..count {
                let previous_delta = self.spring_layouts[index - 1].scroll - target_scroll_y;
                self.spring_chained_targets[index] += previous_delta * LINE_LAYOUT_CHAIN_COUPLING;
            }
        }

        let mut active = false;
        for (index, state) in self.spring_layouts.iter_mut().enumerate() {
            // Focus row and everything above it = rigid block (response 1.0); only
            // rows below the focus soften with distance to form the cascade. A
            // seek glide keeps every row rigid until it settles.
            let response = if !seek_glide && index > focus_end {
                (1.0 - (index - focus_end) as f32 * LINE_LAYOUT_DISTANCE_FALLOFF)
                    .clamp(LINE_LAYOUT_MIN_RESPONSE, 1.0)
            } else {
                1.0
            };
            // Far rows soften (lower stiffness) so they lag and create the
            // cascade, but their damping is scaled by sqrt(response) instead of
            // response. That keeps the damping *ratio* constant across every row
            // (ratio scales with damping / sqrt(stiffness)), so distant rows do a
            // single soft stretch-and-settle like the leading row instead of
            // dropping underdamped and wobbling back and forth.
            let damping_response = response.powf(0.3);
            active |= spring_step(
                &mut state.scroll,
                &mut state.velocity,
                self.spring_chained_targets[index],
                LINE_LAYOUT_SPRING_STIFFNESS * response,
                LINE_LAYOUT_SPRING_DAMPING * damping_response,
                dt,
            );
        }

        self.layout_animation_active = active;
        // The glide is over once the springs reach the target; let the cascade
        // take back over for the next natural focus change.
        if !active {
            self.seek_glide_active = false;
        }
        self.project_from_springs(target_layouts);
    }

    /// Projects `target_layouts` into the reused `frame_layouts` buffer with a
    /// single shared scroll for every row (snap / drag / reset).
    fn project_uniform(&mut self, target_layouts: &[DynamicLineLayout], scroll: f32) {
        self.frame_layouts.clear();
        self.frame_layouts.reserve(target_layouts.len());
        for layout in target_layouts {
            self.frame_layouts.push(DynamicLineLayout {
                top: layout.top - scroll,
                ..*layout
            });
        }
    }

    /// Projects `target_layouts` into the reused `frame_layouts` buffer using each
    /// row's own sprung scroll (the cascade result).
    fn project_from_springs(&mut self, target_layouts: &[DynamicLineLayout]) {
        self.frame_layouts.clear();
        self.frame_layouts.reserve(target_layouts.len());
        for (index, layout) in target_layouts.iter().enumerate() {
            let scroll = self
                .spring_layouts
                .get(index)
                .map(|state| state.scroll)
                .unwrap_or(0.0);
            self.frame_layouts.push(DynamicLineLayout {
                top: layout.top - scroll,
                ..*layout
            });
        }
    }
}

fn spring_step(
    value: &mut f32,
    velocity: &mut f32,
    target: f32,
    stiffness: f32,
    damping: f32,
    dt: f32,
) -> bool {
    let displacement = *value - target;
    let acceleration = -stiffness * displacement - damping * *velocity;
    *velocity += acceleration * dt;
    *value += *velocity * dt;

    if (*value - target).abs() <= LINE_LAYOUT_EPSILON && (*velocity).abs() <= LINE_LAYOUT_EPSILON {
        *value = target;
        *velocity = 0.0;
        false
    } else {
        true
    }
}

fn rubber_band_scroll(raw_scroll_y: f32, max_scroll_y: f32) -> f32 {
    if raw_scroll_y < 0.0 {
        -rubber_band_distance(-raw_scroll_y)
    } else if raw_scroll_y > max_scroll_y {
        max_scroll_y + rubber_band_distance(raw_scroll_y - max_scroll_y)
    } else {
        raw_scroll_y
    }
}

fn rubber_band_distance(distance: f32) -> f32 {
    let distance = distance.max(0.0);
    MANUAL_SCROLL_RUBBER_BAND_LIMIT * distance / (distance + MANUAL_SCROLL_RUBBER_BAND_LIMIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn even_layouts(n: usize, row_height: f32) -> Vec<DynamicLineLayout> {
        (0..n)
            .map(|i| DynamicLineLayout {
                top: i as f32 * row_height,
                height: row_height,
                text_visibility: 1.0,
                interlude_visibility: 0.0,
            })
            .collect()
    }

    // The widest gap between any two rows' scroll offsets in a single frame. Each
    // row's scroll is `content_top - screen_top`; when the list moves as a rigid
    // block every row shares one scroll and the spread is ~0, while a cascade
    // ripple (or a seek "whip") pulls rows to wildly different scrolls.
    fn scroll_spread(projected: &[DynamicLineLayout], content: &[DynamicLineLayout]) -> f32 {
        let scrolls: Vec<f32> = projected
            .iter()
            .zip(content)
            .map(|(p, c)| c.top - p.top)
            .collect();
        let min = scrolls.iter().copied().fold(f32::INFINITY, f32::min);
        let max = scrolls.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        max - min
    }

    // Tapping an earlier, on-screen lyric jumps playback time backward and the
    // focus index from near the bottom up to a small index in one frame. The
    // scroll target moves less than the snap threshold, so the list springs to
    // the new position. Regression guard for the "scroll 乱跳" bug: the spring
    // chain must NOT fire across the seek — otherwise rows that were a rigid block
    // above the old focus become cascade rows below the new focus while still
    // carrying the old, far-away scroll, seeding the cascade with a huge delta
    // that whips the list around. The seek glide keeps every row rigid instead.
    #[test]
    fn backward_seek_glides_without_cascade_whip() {
        let mut renderer = LyricsRenderer::new();
        let content = even_layouts(10, 100.0);
        let viewport = 1000.0;

        // Settle deep into the song: focus at the last row, large scroll, so all
        // rows are one rigid block at scroll = 1000.
        for _ in 0..200 {
            renderer.animate_frame_layout(20_000, &content, 1000.0, viewport, 9);
        }

        // Seek backward to an on-screen line: |200 - 1000| = 800 < 1.6 * viewport,
        // so the list springs rather than snapping.
        let mut worst_spread = 0.0f32;
        for _ in 0..3000 {
            renderer.animate_frame_layout(3_000, &content, 200.0, viewport, 2);
            worst_spread = worst_spread.max(scroll_spread(&renderer.frame_layouts, &content));
        }

        assert!(
            worst_spread < 1.0,
            "backward seek should glide as a rigid block, but rows spread by {worst_spread:.1}px"
        );
    }

    // Natural forward playback (focus advancing line by line, time moving forward
    // by a frame's worth) MUST keep the cascade so the list still ripples. Here a
    // focus change with the springs already near target produces a real lag, so
    // the rows below focus should spread out at least a little.
    #[test]
    fn natural_progression_keeps_cascade_ripple() {
        let mut renderer = LyricsRenderer::new();
        let content = even_layouts(12, 100.0);
        let viewport = 1000.0;

        // Establish a settled block at the top of the song.
        for _ in 0..50 {
            renderer.animate_frame_layout(1_000, &content, 0.0, viewport, 1);
        }

        // Playback advances normally (small forward time step) and the focus moves
        // down with a new scroll target; the cascade should lag the lower rows.
        let mut max_spread = 0.0f32;
        for step in 0..40 {
            let t = 1_000 + step * 30; // ~30ms/frame, well under the seek threshold
            renderer.animate_frame_layout(t, &content, 300.0, viewport, 3);
            max_spread = max_spread.max(scroll_spread(&renderer.frame_layouts, &content));
        }

        assert!(
            max_spread > 1.0,
            "natural progression should still cascade, but rows stayed rigid (spread {max_spread:.1}px)"
        );
    }
}
