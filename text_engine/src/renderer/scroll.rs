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
    /// Set after release/cancel. While this is true the renderer uses one shared
    /// list scroll (no per-line spring). It is cleared only when blur restore
    /// begins and the current manual scroll is handed to the line spring.
    return_to_auto_requested: bool,
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
        self.last_seek_detection_playback_ms = None;
        self.last_target_scroll_y = None;
        self.pending_lyric_click_seek = None;
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
        self.manual_scroll.return_to_auto_requested = false;
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
        self.manual_scroll.dragging = true;
        self.manual_scroll.return_to_auto_requested = false;
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
        self.manual_scroll.return_to_auto_requested = true;
        self.manual_scroll_active = true;
        self.last_manual_scroll_frame_at = Some(now);
        self.engage_manual_scroll_blur(now);
    }

    pub fn cancel_manual_scroll(&mut self) {
        let now = Instant::now();
        self.manual_scroll.dragging = false;
        self.manual_scroll.velocity = 0.0;
        self.manual_scroll.return_to_auto_requested = true;
        self.manual_scroll_active = self.manual_scroll.offset.abs() > LINE_LAYOUT_EPSILON;
        self.engage_manual_scroll_blur(now);
    }

    /// Prepares state for a discontinuous playback-time jump. A pending lyric
    /// click means the user tapped a line that was already on screen, possibly
    /// after manually scrolling far away from the current auto position. In that
    /// case the spring must start from the visible list scroll recorded during
    /// hit-test, not from the old auto target; otherwise the distance reset logic
    /// misclassifies the tap as an off-range scrub and snaps.
    pub(super) fn prepare_seek_transition(
        &mut self,
        current_time_ms: i32,
        target_layout_count: usize,
    ) {
        let now = Instant::now();
        if self
            .pending_lyric_click_seek
            .is_some_and(|pending| now.duration_since(pending.recorded_at)
                > Duration::from_millis(LYRIC_CLICK_SEEK_PENDING_MS))
        {
            self.pending_lyric_click_seek = None;
        }

        let seek_landed = self.last_seek_detection_playback_ms.is_some_and(|last| {
            let delta = current_time_ms - last;
            delta < -LINE_LAYOUT_SEEK_BACKWARD_MS || delta > LINE_LAYOUT_SEEK_FORWARD_MS
        });
        self.last_seek_detection_playback_ms = Some(current_time_ms);

        if !seek_landed || self.manual_scroll.dragging {
            return;
        }

        if let Some(pending) = self.pending_lyric_click_seek.take() {
            if self.pending_click_matches_time(pending, current_time_ms) {
                self.seed_lyric_click_seek(
                    pending.visible_scroll_y,
                    current_time_ms,
                    target_layout_count,
                );
                self.reset_manual_scroll();
                return;
            }
        }

        if !self.manual_scroll.return_to_auto_requested {
            self.reset_manual_scroll();
        }
    }

    pub(super) fn update_manual_scroll_target(
        &mut self,
        current_time_ms: i32,
        auto_scroll_y: f32,
        max_scroll_y: f32,
        target_layout_count: usize,
    ) -> f32 {
        let now = Instant::now();
        let dt = self
            .last_manual_scroll_frame_at
            .map(|last| now.duration_since(last).as_secs_f32())
            .unwrap_or(0.0)
            .clamp(0.001, LINE_LAYOUT_MAX_DT);
        self.last_manual_scroll_frame_at = Some(now);

        let blur_engaged = self.manual_scroll.dragging
            || self
                .manual_scroll
                .blur_engaged_until
                .is_some_and(|until| now < until);
        let mut active = self.manual_scroll.dragging;
        if !self.manual_scroll.dragging {
            if self.manual_scroll.return_to_auto_requested && !blur_engaged {
                let return_scroll_y =
                    self.manual_scroll_projected_scroll(auto_scroll_y, max_scroll_y);
                let return_displacement = return_scroll_y - auto_scroll_y;
                let can_finish_return = return_displacement.abs() <= LINE_LAYOUT_EPSILON
                    || self.seed_manual_scroll_return_glide(
                        return_scroll_y,
                        current_time_ms,
                        auto_scroll_y,
                        target_layout_count,
                    );
                if can_finish_return {
                    self.manual_scroll.offset = 0.0;
                    self.manual_scroll.velocity = 0.0;
                    self.manual_scroll.return_to_auto_requested = false;
                    active |= return_displacement.abs() > LINE_LAYOUT_EPSILON;
                } else {
                    active = true;
                }
                if can_finish_return {
                    self.manual_scroll_active = true;
                    self.update_manual_scroll_blur(now, dt, blur_engaged);
                    return auto_scroll_y;
                }
            } else if self.manual_scroll.velocity.abs() > MANUAL_SCROLL_VELOCITY_EPSILON {
                self.manual_scroll.offset += self.manual_scroll.velocity * dt;
                // iOS exponential deceleration: v *= rate^(elapsed_ms).
                self.manual_scroll.velocity *= MANUAL_SCROLL_DECELERATION_RATE.powf(dt * 1000.0);
                if self.manual_scroll.velocity.abs() <= MANUAL_SCROLL_VELOCITY_EPSILON {
                    self.manual_scroll.velocity = 0.0;
                }
                active = true;
            }

            let lower_offset = -auto_scroll_y;
            let upper_offset = max_scroll_y - auto_scroll_y;
            let bounded_offset = self.manual_scroll.offset.clamp(lower_offset, upper_offset);
            let overscrolled =
                (self.manual_scroll.offset - bounded_offset).abs() > LINE_LAYOUT_EPSILON;
            if overscrolled {
                active |= spring_step(
                    &mut self.manual_scroll.offset,
                    &mut self.manual_scroll.velocity,
                    bounded_offset,
                    MANUAL_SCROLL_OVERSCROLL_STIFFNESS,
                    MANUAL_SCROLL_OVERSCROLL_DAMPING,
                    dt,
                );
            } else if self.manual_scroll.velocity.abs() <= MANUAL_SCROLL_VELOCITY_EPSILON
            {
                self.manual_scroll.velocity = 0.0;
            }
        }

        let mut manual_scroll_active = active
            || self.manual_scroll.dragging
            || self.manual_scroll.return_to_auto_requested
            || self.manual_scroll.offset.abs() > LINE_LAYOUT_EPSILON
            || self.manual_scroll.velocity.abs() > MANUAL_SCROLL_VELOCITY_EPSILON;

        // Depth-of-field blur is released while the finger is down and for a
        // fixed window after the last touch input, then eases back in. This is a
        // pure, monotonic timer driven only by touch events — the fling/return
        // physics never touch `blur_engaged_until`, so the automatic glide-back
        // to the active line (or normal playback auto-scroll) can never flip the
        // blur off/on again. Keep rendering while the blur is engaged or fading
        // so the ease-in still runs even when playback is paused.
        if self.update_manual_scroll_blur(now, dt, blur_engaged) {
            manual_scroll_active = true;
        }

        self.manual_scroll_active = manual_scroll_active;
        self.manual_scroll_projected_scroll(auto_scroll_y, max_scroll_y)
    }

    fn update_manual_scroll_blur(&mut self, now: Instant, dt: f32, blur_engaged: bool) -> bool {
        let blur_target = if blur_engaged { 1.0 } else { 0.0 };
        let mut active = blur_engaged;
        if (self.manual_scroll_blur_release - blur_target).abs() > 0.001 {
            let rate = if blur_target > self.manual_scroll_blur_release {
                MANUAL_SCROLL_BLUR_FADE_OUT_RATE
            } else {
                MANUAL_SCROLL_BLUR_FADE_IN_RATE
            };
            let factor = 1.0 - (-rate * dt).exp();
            self.manual_scroll_blur_release +=
                (blur_target - self.manual_scroll_blur_release) * factor;
            if (self.manual_scroll_blur_release - blur_target).abs() <= 0.001 {
                self.manual_scroll_blur_release = blur_target;
            } else {
                active = true;
            }
        }
        if self
            .manual_scroll
            .blur_engaged_until
            .is_some_and(|until| now >= until)
        {
            self.manual_scroll.blur_engaged_until = None;
        }
        active
    }

    pub(super) fn manual_scroll_plain_list_active(&self) -> bool {
        self.manual_scroll.dragging || self.manual_scroll.return_to_auto_requested
    }

    pub(super) fn manual_scroll_projected_scroll(
        &self,
        auto_scroll_y: f32,
        max_scroll_y: f32,
    ) -> f32 {
        let raw_scroll_y = auto_scroll_y + self.manual_scroll.offset;
        rubber_band_scroll(raw_scroll_y, max_scroll_y)
    }

    fn seed_manual_scroll_return_glide(
        &mut self,
        start_scroll_y: f32,
        current_time_ms: i32,
        target_scroll_y: f32,
        target_layout_count: usize,
    ) -> bool {
        if target_layout_count == 0 {
            return false;
        }

        if self.spring_layouts.len() != target_layout_count {
            self.spring_layouts = vec![
                SpringLineState {
                    scroll: start_scroll_y,
                    velocity: 0.0,
                };
                target_layout_count
            ];
        }
        for state in &mut self.spring_layouts {
            state.scroll = start_scroll_y;
            state.velocity = 0.0;
        }
        // Use the same rigid-block spring path as a seek glide: the manual view is
        // treated as the starting scroll position, and the existing auto target is
        // chased by the per-line spring system instead of by manual offset decay.
        self.seek_glide_active = true;
        self.layout_animation_active = true;
        self.last_spring_frame_at = Some(Instant::now());
        self.last_spring_playback_ms = Some(current_time_ms);
        self.last_target_scroll_y = Some(target_scroll_y);
        true
    }

    fn seed_lyric_click_seek(
        &mut self,
        start_scroll_y: f32,
        current_time_ms: i32,
        target_layout_count: usize,
    ) -> bool {
        if target_layout_count == 0 {
            return false;
        }

        if self.spring_layouts.len() != target_layout_count {
            self.spring_layouts = vec![
                SpringLineState {
                    scroll: start_scroll_y,
                    velocity: 0.0,
                };
                target_layout_count
            ];
        }
        for state in &mut self.spring_layouts {
            state.scroll = start_scroll_y;
            state.velocity = 0.0;
        }

        self.seek_glide_active = false;
        self.layout_animation_active = true;
        self.last_spring_frame_at = Some(Instant::now());
        // This seek is already classified by hit-test, so suppress the generic
        // playback-delta seek glide. The next spring pass should behave like a
        // normal on-screen lyric click: focused row/above are rigid, lower rows
        // cascade from that first spring.
        self.last_spring_playback_ms = Some(current_time_ms);
        self.last_target_scroll_y = Some(start_scroll_y);
        true
    }

    fn pending_click_matches_time(
        &self,
        pending: PendingLyricClickSeek,
        current_time_ms: i32,
    ) -> bool {
        let Some(scene) = &self.scene else {
            return true;
        };
        scene.lines.iter().any(|line| {
            line.source_index == pending.source_index
                && current_time_ms >= line.start - LINE_LAYOUT_SEEK_FORWARD_MS
                && current_time_ms <= line.effective_end + LINE_LAYOUT_SEEK_FORWARD_MS
        })
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
        // jump further than the spring range we intentionally animate.
        let seek_reset_distance =
            (viewport_height * LINE_LAYOUT_SEEK_RESET_DISTANCE_FACTOR).max(1.0);
        let scroll_jump = self
            .last_target_scroll_y
            .map(|last| (target_scroll_y - last).abs());
        let playback_seek = self.last_spring_playback_ms.is_some_and(|last| {
            let delta = current_time_ms - last;
            delta < -LINE_LAYOUT_SEEK_BACKWARD_MS || delta > LINE_LAYOUT_SEEK_FORWARD_MS
        });
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
            self.project_uniform_frame_layout(target_layouts, target_scroll_y);
            return;
        }

        let dt = self
            .last_spring_frame_at
            .map(|last| now.duration_since(last).as_secs_f32())
            .unwrap_or(0.0)
            .clamp(0.001, LINE_LAYOUT_MAX_DT);
        self.last_spring_frame_at = Some(now);
        // Any discontinuous playback-time jump is a seek, not natural playback.
        // Glide it as one rigid block: large focus-index jumps can otherwise move
        // rows between the rigid and cascade regions while they still carry stale
        // scroll state, which makes off-range clicks whip the list around. Normal
        // frame-by-frame playback keeps the cascade below.
        if playback_seek {
            self.seek_glide_active = true;
        }
        self.last_spring_playback_ms = Some(current_time_ms);
        self.last_target_scroll_y = Some(target_scroll_y);

        // NOTE: this spring cascade tracks the pure AUTO-scroll target only. The
        // manual-scroll offset (drag / fling / return) is NOT folded in here — the
        // caller adds it as a flat per-frame shift on top of the projected layout.
        // That keeps the manual gesture perfectly 1:1 (responsive fling) while the
        // inter-line ripple keeps running underneath, so it's still there the
        // instant auto-scroll resumes after a manual scroll.

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
    pub(super) fn project_uniform_frame_layout(
        &mut self,
        target_layouts: &[DynamicLineLayout],
        scroll: f32,
    ) {
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
    // iOS rubber-band damping (ktiays/fluid-scroll): the further you pull past the
    // edge, the harder it resists, asymptoting to `limit`. `c` softens the initial
    // resistance so the first bit of overscroll still moves with the finger.
    let distance = distance.max(0.0);
    let limit = MANUAL_SCROLL_RUBBER_BAND_LIMIT;
    (1.0 - 1.0 / (distance / limit * MANUAL_SCROLL_RUBBER_BAND_COEFFICIENT + 1.0)) * limit
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

    fn set_pending_click(renderer: &mut LyricsRenderer, source_index: usize, visible_scroll_y: f32) {
        renderer.pending_lyric_click_seek = Some(PendingLyricClickSeek {
            source_index,
            visible_scroll_y,
            recorded_at: Instant::now(),
        });
    }

    #[test]
    fn pending_manual_return_waits_for_blur_restore_before_gliding_back() {
        let mut renderer = LyricsRenderer::new();
        renderer.manual_scroll.offset = 360.0;
        renderer.manual_scroll.velocity = 0.0;
        renderer.manual_scroll.dragging = false;
        renderer.manual_scroll.return_to_auto_requested = true;
        renderer.manual_scroll.blur_engaged_until = Some(Instant::now() + Duration::from_secs(60));
        renderer.last_manual_scroll_frame_at = Some(Instant::now() - Duration::from_millis(16));

        let projected = renderer.update_manual_scroll_target(1_000, 480.0, 1200.0, 6);

        assert_eq!(projected, 840.0);
        assert_eq!(renderer.manual_scroll.offset, 360.0);
        assert_eq!(renderer.manual_scroll.velocity, 0.0);
        assert!(renderer.manual_scroll.return_to_auto_requested);
        assert!(renderer.manual_scroll_plain_list_active());
        assert!(!renderer.seek_glide_active);
    }

    #[test]
    fn ending_manual_scroll_keeps_fling_until_blur_restore_window_expires() {
        let mut renderer = LyricsRenderer::new();
        let content = even_layouts(6, 100.0);
        let viewport = 600.0;

        renderer.animate_frame_layout(1_000, &content, 200.0, viewport, 2);
        renderer.begin_manual_scroll();
        renderer.scroll_manual_by(120.0);
        renderer.end_manual_scroll(1_000.0);

        let projected = renderer.update_manual_scroll_target(1_016, 200.0, 900.0, content.len());

        assert!(projected > 320.0);
        assert!(renderer.manual_scroll.offset > 120.0);
        assert!(renderer.manual_scroll.velocity > 0.0);
        assert!(renderer.manual_scroll.return_to_auto_requested);
        assert!(renderer.manual_scroll_plain_list_active());
        assert!(!renderer.seek_glide_active);
    }

    #[test]
    fn manual_scroll_projects_as_plain_list_without_line_spring() {
        let mut renderer = LyricsRenderer::new();
        let content = even_layouts(6, 100.0);
        let viewport = 600.0;

        for _ in 0..20 {
            renderer.animate_frame_layout(1_000, &content, 200.0, viewport, 2);
        }
        renderer.begin_manual_scroll();
        renderer.scroll_manual_by(120.0);

        let combined_scroll_y =
            renderer.update_manual_scroll_target(1_016, 200.0, 900.0, content.len());
        assert!(renderer.manual_scroll_plain_list_active());

        renderer.project_uniform_frame_layout(&content, combined_scroll_y);

        assert!(
            scroll_spread(&renderer.frame_layouts, &content) < LINE_LAYOUT_EPSILON,
            "manual scroll should move every row with one shared list scroll"
        );
        assert!((renderer.frame_layouts[0].top + 320.0).abs() < LINE_LAYOUT_EPSILON);
    }

    #[test]
    fn normal_playback_after_release_keeps_pending_manual_return() {
        let mut renderer = LyricsRenderer::new();
        let content = even_layouts(6, 100.0);
        let viewport = 600.0;

        renderer.animate_frame_layout(1_000, &content, 200.0, viewport, 2);
        renderer.prepare_seek_transition(1_000, content.len());
        renderer.begin_manual_scroll();
        renderer.scroll_manual_by(120.0);
        renderer.prepare_seek_transition(1_030, content.len());
        renderer.end_manual_scroll(0.0);
        renderer.prepare_seek_transition(1_060, content.len());

        assert!(renderer.manual_scroll.return_to_auto_requested);
        assert_eq!(renderer.manual_scroll.offset, 120.0);
        assert!(renderer.manual_scroll_plain_list_active());
    }

    #[test]
    fn seek_during_pending_manual_return_clears_plain_list_state() {
        let mut renderer = LyricsRenderer::new();
        let content = even_layouts(6, 100.0);
        let viewport = 600.0;

        renderer.animate_frame_layout(1_000, &content, 200.0, viewport, 2);
        renderer.prepare_seek_transition(1_000, content.len());
        renderer.begin_manual_scroll();
        renderer.scroll_manual_by(120.0);
        renderer.end_manual_scroll(0.0);
        set_pending_click(&mut renderer, 0, 320.0);
        renderer.prepare_seek_transition(2_000, content.len());

        assert!(!renderer.manual_scroll.return_to_auto_requested);
        assert_eq!(renderer.manual_scroll.offset, 0.0);
        assert!(!renderer.manual_scroll_plain_list_active());
        assert!(!renderer.seek_glide_active);
    }

    #[test]
    fn forward_tap_seek_keeps_spring_animation_after_manual_changes() {
        let mut renderer = LyricsRenderer::new();
        let content = even_layouts(8, 100.0);
        let viewport = 600.0;

        renderer.animate_frame_layout(1_000, &content, 0.0, viewport, 1);
        renderer.prepare_seek_transition(1_000, content.len());
        set_pending_click(&mut renderer, 0, 0.0);
        renderer.prepare_seek_transition(3_000, content.len());
        let combined_scroll_y =
            renderer.update_manual_scroll_target(3_000, 300.0, 900.0, content.len());
        assert_eq!(combined_scroll_y, 300.0);
        assert!(!renderer.manual_scroll_plain_list_active());

        renderer.animate_frame_layout(3_000, &content, 300.0, viewport, 3);

        let first_scroll = content[0].top - renderer.frame_layouts[0].top;
        assert!(
            first_scroll > 0.0 && first_scroll < 300.0,
            "tap seek should spring toward target, not snap directly to {first_scroll}"
        );
        assert!(renderer.layout_animation_active);
        assert!(!renderer.seek_glide_active);
    }

    #[test]
    fn forward_seek_with_far_focus_jump_glides_without_cascade_whip() {
        let mut renderer = LyricsRenderer::new();
        let content = even_layouts(120, 100.0);
        let viewport = 1000.0;

        for _ in 0..200 {
            renderer.animate_frame_layout(20_000, &content, 7_800.0, viewport, 78);
        }

        // The scroll delta stays under the snap threshold (1.6 * viewport), but
        // the focus index jumps far enough that the normal cascade would reclassify
        // many rows with stale scroll state. A seek must glide as a rigid block.
        let mut worst_spread = 0.0f32;
        for _ in 0..120 {
            renderer.animate_frame_layout(35_000, &content, 9_000.0, viewport, 100);
            worst_spread = worst_spread.max(scroll_spread(&renderer.frame_layouts, &content));
        }

        assert!(
            worst_spread < 1.0,
            "forward off-range seek should glide as a rigid block, but rows spread by {worst_spread:.1}px"
        );
    }

    #[test]
    fn unclassified_far_jump_still_snaps_instead_of_using_click_seek_path() {
        let mut renderer = LyricsRenderer::new();
        let content = even_layouts(120, 100.0);
        let viewport = 1000.0;

        renderer.animate_frame_layout(1_000, &content, 0.0, viewport, 1);
        renderer.prepare_seek_transition(1_000, content.len());
        renderer.prepare_seek_transition(20_000, content.len());
        renderer.animate_frame_layout(20_000, &content, 5_000.0, viewport, 50);

        let first_scroll = content[0].top - renderer.frame_layouts[0].top;
        assert_eq!(first_scroll, 5_000.0);
        assert!(!renderer.layout_animation_active);
        assert!(!renderer.seek_glide_active);
    }

    #[test]
    fn manually_revealed_far_forward_click_uses_visible_scroll_as_seek_start() {
        let mut renderer = LyricsRenderer::new();
        let content = even_layouts(120, 100.0);
        let viewport = 1000.0;

        renderer.animate_frame_layout(1_000, &content, 0.0, viewport, 1);
        renderer.prepare_seek_transition(1_000, content.len());
        renderer.manual_scroll.offset = 4_900.0;
        renderer.manual_scroll.return_to_auto_requested = true;
        renderer.manual_scroll_active = true;
        set_pending_click(&mut renderer, 50, 4_900.0);

        renderer.prepare_seek_transition(20_000, content.len());

        assert_eq!(renderer.manual_scroll.offset, 0.0);
        assert!(!renderer.manual_scroll_plain_list_active());
        assert!(!renderer.seek_glide_active);
        assert!(
            renderer
                .spring_layouts
                .iter()
                .all(|state| (state.scroll - 4_900.0).abs() <= LINE_LAYOUT_EPSILON)
        );

        renderer.last_spring_frame_at = Some(Instant::now() - Duration::from_millis(16));
        renderer.animate_frame_layout(20_000, &content, 5_000.0, viewport, 50);

        let first_scroll = content[0].top - renderer.frame_layouts[0].top;
        assert!(
            first_scroll > 4_900.0 && first_scroll < 5_000.0,
            "manual far forward click should start from visible scroll, got {first_scroll}"
        );
        assert!(
            scroll_spread(&renderer.frame_layouts, &content) > 0.5,
            "manual far forward click should use normal cascade, not global rigid glide"
        );
    }

    #[test]
    fn manually_revealed_far_backward_click_uses_visible_scroll_as_seek_start() {
        let mut renderer = LyricsRenderer::new();
        let content = even_layouts(120, 100.0);
        let viewport = 1000.0;

        renderer.animate_frame_layout(20_000, &content, 8_000.0, viewport, 80);
        renderer.prepare_seek_transition(20_000, content.len());
        renderer.manual_scroll.offset = -2_900.0;
        renderer.manual_scroll.return_to_auto_requested = true;
        renderer.manual_scroll_active = true;
        set_pending_click(&mut renderer, 50, 5_100.0);

        renderer.prepare_seek_transition(10_000, content.len());

        assert_eq!(renderer.manual_scroll.offset, 0.0);
        assert!(!renderer.manual_scroll_plain_list_active());
        assert!(!renderer.seek_glide_active);
        assert!(
            renderer
                .spring_layouts
                .iter()
                .all(|state| (state.scroll - 5_100.0).abs() <= LINE_LAYOUT_EPSILON)
        );

        renderer.last_spring_frame_at = Some(Instant::now() - Duration::from_millis(16));
        renderer.animate_frame_layout(10_000, &content, 5_000.0, viewport, 50);

        let first_scroll = content[0].top - renderer.frame_layouts[0].top;
        assert!(
            first_scroll > 5_000.0 && first_scroll < 5_100.0,
            "manual far backward click should start from visible scroll, got {first_scroll}"
        );
        assert!(
            scroll_spread(&renderer.frame_layouts, &content) > 0.5,
            "manual far backward click should use normal cascade, not global rigid glide"
        );
    }

    #[test]
    fn manual_scroll_return_transfers_offset_to_layout_spring_when_blur_restores() {
        let mut renderer = LyricsRenderer::new();
        let content = even_layouts(6, 100.0);
        let viewport = 600.0;

        renderer.animate_frame_layout(1_000, &content, 200.0, viewport, 2);
        renderer.begin_manual_scroll();
        renderer.scroll_manual_by(120.0);
        renderer.end_manual_scroll(0.0);
        renderer.manual_scroll.blur_engaged_until = Some(Instant::now() - Duration::from_millis(1));
        renderer.last_manual_scroll_frame_at = Some(Instant::now() - Duration::from_millis(16));

        let projected = renderer.update_manual_scroll_target(1_016, 200.0, 900.0, content.len());

        assert_eq!(projected, 200.0);
        assert_eq!(renderer.manual_scroll.offset, 0.0);
        assert_eq!(renderer.manual_scroll.velocity, 0.0);
        assert!(!renderer.manual_scroll.return_to_auto_requested);
        assert!(renderer.seek_glide_active);
        assert!(
            renderer
                .spring_layouts
                .iter()
                .all(|state| (state.scroll - 320.0).abs() <= LINE_LAYOUT_EPSILON)
        );

        renderer.animate_frame_layout(1_016, &content, 200.0, viewport, 2);

        assert!(renderer.frame_layouts[0].top < -200.0 - LINE_LAYOUT_EPSILON);
        assert!(
            scroll_spread(&renderer.frame_layouts, &content) < 1.0,
            "manual release return should glide as a rigid click-style spring"
        );
    }

    #[test]
    fn manual_scroll_return_seeds_glide_even_without_existing_spring_state() {
        let mut renderer = LyricsRenderer::new();
        let content = even_layouts(6, 100.0);
        let viewport = 600.0;

        renderer.manual_scroll.offset = 120.0;
        renderer.manual_scroll.dragging = false;
        renderer.manual_scroll.return_to_auto_requested = true;
        renderer.manual_scroll.blur_engaged_until = Some(Instant::now() - Duration::from_millis(1));
        renderer.last_manual_scroll_frame_at = Some(Instant::now() - Duration::from_millis(16));

        let projected = renderer.update_manual_scroll_target(1_000, 200.0, 900.0, content.len());

        assert_eq!(projected, 200.0);
        assert_eq!(renderer.spring_layouts.len(), content.len());
        assert!(renderer.seek_glide_active);
        assert!(
            renderer
                .spring_layouts
                .iter()
                .all(|state| (state.scroll - 320.0).abs() <= LINE_LAYOUT_EPSILON)
        );

        renderer.animate_frame_layout(1_000, &content, 200.0, viewport, 2);

        assert!(renderer.frame_layouts[0].top < -200.0 - LINE_LAYOUT_EPSILON);
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
