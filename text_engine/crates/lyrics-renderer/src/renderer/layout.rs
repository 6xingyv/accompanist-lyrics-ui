//! Scene preparation & text layout: turning a `LyricsScene` into positioned,
//! shaped `PreparedScene` rows — karaoke syllable measurement, line balancing /
//! greedy wrapping, and per-cluster font-span shaping. Split out of `renderer.rs`.

use super::*;

const WIDE_MIN_ASPECT_RATIO: f32 = 1.3;
const WIDE_MIN_WIDTH: f32 = 1024.0;
const WIDE_MAX_SCALE_WIDTH: f32 = 1600.0;
const WIDE_MIN_SCALE_HEIGHT: f32 = 512.0;
const WIDE_MAX_SCALE_HEIGHT: f32 = 1000.0;
const WIDE_TEXT_SCALE: f32 = 1.2;
const WIDE_MAX_LYRICS_LAYOUT_SCALE: f32 = 1.4;
const WIDE_FOCUS_Y_RATIO: f32 = 0.4;

#[inline]
pub(super) fn wide_lyrics_layout_scale(width: f32, height: f32) -> f32 {
    let width_progress = ((width - WIDE_MIN_WIDTH) / (WIDE_MAX_SCALE_WIDTH - WIDE_MIN_WIDTH))
        .clamp(0.0, 1.0);
    let height_progress =
        ((height - WIDE_MIN_SCALE_HEIGHT) / (WIDE_MAX_SCALE_HEIGHT - WIDE_MIN_SCALE_HEIGHT))
            .clamp(0.0, 1.0);
    let progress = width_progress.min(height_progress);
    1.0 + progress * (WIDE_MAX_LYRICS_LAYOUT_SCALE - 1.0)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ResolvedPlayerChrome {
    pub(super) content_top: f32,
    pub(super) content_bottom: f32,
    pub(super) content_left: f32,
    pub(super) content_right: f32,
    pub(super) lyrics_clip_left: f32,
    pub(super) lyrics_clip_right: f32,
    pub(super) landscape_player: bool,
    pub(super) lyrics_layout_scale: f32,
    pub(super) focus_y: Option<f32>,
    pub(super) thumb_border_width: f32,
}

pub(super) fn resolve_keep_alive(
    chrome: &ResolvedPlayerChrome,
    spacing: &SpacingInput,
) -> f32 {
    chrome
        .focus_y
        .map(|focus_y| {
            (focus_y - spacing.line_padding * chrome.lyrics_layout_scale)
                .max(chrome.content_top)
        })
        .unwrap_or(spacing.focus_top_offset + chrome.content_top)
}

/// Resolve landscape lyric scaling for every host, then optionally expand player
/// chrome. Android scenes may omit top-bar geometry; desktop/player scenes that do
/// provide it and match LyricsBlossom's decompiled Wide condition are reshaped into
/// the fully-expanded default layout (`mode != 1`).
pub(super) fn resolve_player_chrome(scene: &mut LyricsScene) -> ResolvedPlayerChrome {
    let width = scene.width.unwrap_or(DEFAULT_WIDTH).max(DEFAULT_WIDTH) as f32;
    let height = scene.height.unwrap_or(DEFAULT_HEIGHT).max(DEFAULT_HEIGHT) as f32;
    let safe_left = scene.content_left.unwrap_or(0.0).max(0.0);
    let safe_right = scene.content_right.unwrap_or(0.0).max(0.0);
    let aspect_ratio = width / height.max(1.0);
    let layout_density = scene.layout_density.unwrap_or(1.0).clamp(0.25, 8.0);
    let scale_basis_width = width / layout_density;
    let scale_basis_height = height / layout_density;
    // Lyrics scaling/focus belongs to the viewport, not to desktop/player chrome.
    // Android intentionally may omit the in-surface top bar, but its Rust-rendered
    // landscape lyrics must still use the same dynamic 1.0→1.4 scale. The top-bar
    // condition remains only on the two-pane cover/metadata layout below.
    let landscape_lyrics = aspect_ratio >= WIDE_MIN_ASPECT_RATIO && width >= WIDE_MIN_WIDTH;
    let landscape_player = scene.top_bar.is_some() && landscape_lyrics;
    let mut chrome = ResolvedPlayerChrome {
        content_top: scene.content_top.unwrap_or(0.0).max(0.0),
        content_bottom: scene.content_bottom.unwrap_or(0.0).max(0.0),
        content_left: safe_left,
        content_right: safe_right,
        lyrics_clip_left: safe_left,
        lyrics_clip_right: safe_right,
        landscape_player,
        lyrics_layout_scale: if landscape_lyrics {
            wide_lyrics_layout_scale(scale_basis_width, scale_basis_height)
        } else {
            1.0
        },
        focus_y: landscape_lyrics.then_some(height * WIDE_FOCUS_Y_RATIO),
        thumb_border_width: 0.0,
    };
    if !chrome.landscape_player {
        return chrome;
    }

    let Some(bar) = scene.top_bar.as_mut() else {
        return chrome;
    };

    // Both Android and desktop build the compact input with a 68dp thumbnail, so
    // it also carries the effective px-per-dp/render scale needed by the decompiled
    // `95*s`, `16*s*1.2`, and `500*s*1.2` constants.
    let scale = (bar.thumb_size / 68.0).clamp(0.25, 8.0);
    let dp = |value: f32| value * scale;
    let system_top = (bar.thumb_top - dp(28.0)).max(0.0);
    // Android renders the lyrics into an isolated, blurred layer. Its glyph ink,
    // per-word scale and blur can extend past the nominal text frame; clipping at
    // the decompiled viewport edge cuts that overflow into a hard vertical edge.
    // Keep desktop geometry exact (layoutDensity omitted/1), while Android receives
    // 24dp of transparent clip bleed on both sides. This does not change wrapping.
    let clip_bleed = if layout_density > 1.01 { dp(24.0) } else { 0.0 };

    // Decompiled default Wide *layout* viewport: [0.50W - 18, 0.96W]. Keep these
    // nominal edges separate from the expanded Canvas clip below: using the bleed
    // width for wrapping makes lines believe they have more room than the column and
    // is exactly what causes the final glyphs to be truncated at the visible edge.
    let lyrics_viewport_left = (width * 0.50 - 18.0).max(safe_left);
    let lyrics_viewport_right_edge = (width * 0.96)
        .min(width - safe_right)
        .max(lyrics_viewport_left + 1.0)
        .min(width);
    let lyrics_viewport_width =
        (lyrics_viewport_right_edge - lyrics_viewport_left).max(1.0);
    let lyrics_clip_left = (lyrics_viewport_left - clip_bleed).max(safe_left);
    let lyrics_clip_right_edge =
        (lyrics_viewport_right_edge + clip_bleed).min(width - safe_right);

    // Decompiled inner text frame: centered in the viewport, padded by 16*s*1.2,
    // and capped at 500*s*1.2. SceneConfig's content insets exclude the renderer's
    // own line padding so the resulting text origin/wrap width match this frame.
    let inner_padding = dp(16.0) * WIDE_TEXT_SCALE;
    let max_text_width = dp(500.0) * WIDE_TEXT_SCALE;
    let text_width = (lyrics_viewport_width - inner_padding * 2.0)
        .max(1.0)
        .min(max_text_width);
    let text_left = lyrics_viewport_left + (lyrics_viewport_width - text_width) * 0.5;
    let line_padding_x =
        scene.style.spacing.horizontal_padding.max(0.0) * chrome.lyrics_layout_scale;

    chrome.content_top = system_top;
    chrome.content_left = (text_left - line_padding_x).max(0.0);
    chrome.content_right =
        (width - (text_left + text_width) - line_padding_x).max(0.0);
    chrome.lyrics_clip_left = lyrics_clip_left;
    chrome.lyrics_clip_right = (width - lyrics_clip_right_edge).max(0.0);
    chrome.thumb_border_width = dp(1.0);

    // LyricsBlossom's Wide cover is 0.5H and leaves 0.05H before metadata. This
    // renderer currently has only cover + metadata (not the original progress /
    // transport / volume stack), so center that visible group as a unit. Keeping
    // the old absolute 0.12H/0.67H anchors made short ultrawide windows hug the
    // top-left and could pull metadata back over the cover via the 95*s reserve.
    let button_radius = bar.button_radius.max(dp(1.0));
    let text_block_height = bar.title_line_height + bar.artist_line_height;
    let metadata_row_height = text_block_height.max(button_radius * 2.0);
    let metadata_gap = height * 0.05;
    let available_height = (height - system_top - chrome.content_bottom).max(1.0);
    let left_pane_width = (lyrics_clip_left - safe_left).max(1.0);
    let cover_size = (height * 0.50)
        .min((available_height - metadata_gap - metadata_row_height).max(1.0))
        .min(left_pane_width);
    let group_height = cover_size + metadata_gap + metadata_row_height;
    let cover_left = safe_left + (left_pane_width - cover_size) * 0.5;
    let cover_top = system_top + ((available_height - group_height) * 0.5).max(0.0);
    let metadata_row_top = cover_top + cover_size + metadata_gap;

    bar.thumb_left = cover_left;
    bar.thumb_top = cover_top;
    bar.thumb_size = cover_size;
    bar.thumb_radius = dp(12.0).min(cover_size * 0.5);
    bar.text_left = cover_left;
    bar.title_top = metadata_row_top + (metadata_row_height - text_block_height) * 0.5;
    bar.artist_top = bar.title_top + bar.title_line_height;
    bar.button_cx = cover_left + cover_size - button_radius;
    bar.button_cy = metadata_row_top + metadata_row_height * 0.5;
    bar.text_max_width = (bar.button_cx - button_radius - dp(8.0) - bar.text_left).max(1.0);

    chrome
}

impl LyricsRenderer {
    pub(super) fn prepare_scene(&mut self, scene: LyricsScene) -> Result<PreparedScene, String> {
        let mut scene = scene;
        let locale = scene.locale.as_deref().unwrap_or("en-US");
        self.set_locale(locale);

        let chrome = resolve_player_chrome(&mut scene);

        // All style defaults now live in the wire `*Input` structs' `Default`
        // impls, so this just copies the (already-defaulted) values across and
        // applies the engine's validation clamps.
        let style = &scene.style;
        let typography = &style.typography;
        let spacing = &style.spacing;
        let dots = &style.breathing_dots;
        let spring = &style.auto_scroll_spring;
        let manual = &style.manual_scroll;
        let lyrics_layout_scale = chrome.lyrics_layout_scale;
        let config = SceneConfig {
            width: scene.width.unwrap_or(DEFAULT_WIDTH).max(DEFAULT_WIDTH),
            height: scene.height.unwrap_or(DEFAULT_HEIGHT).max(DEFAULT_HEIGHT),
            landscape_player: chrome.landscape_player,
            normal_font_size: typography.normal_font_size * lyrics_layout_scale,
            normal_line_height: typography.normal_line_height * lyrics_layout_scale,
            normal_attrs: TextAttrs {
                weight: typography.normal_font_weight,
                italic: typography.normal_font_italic,
            },
            accompaniment_font_size: typography.accompaniment_font_size * lyrics_layout_scale,
            accompaniment_line_height: typography.accompaniment_line_height
                * lyrics_layout_scale,
            accompaniment_attrs: TextAttrs {
                weight: typography.accompaniment_font_weight,
                italic: typography.accompaniment_font_italic,
            },
            translation_font_size: typography.translation_font_size * lyrics_layout_scale,
            translation_line_height: typography.translation_line_height * lyrics_layout_scale,
            translation_attrs: TextAttrs {
                weight: typography.translation_font_weight,
                italic: typography.translation_font_italic,
            },
            accompaniment_translation_font_size: typography.accompaniment_translation_font_size
                * lyrics_layout_scale,
            accompaniment_translation_line_height: typography
                .accompaniment_translation_line_height
                * lyrics_layout_scale,
            accompaniment_translation_attrs: TextAttrs {
                weight: typography.accompaniment_translation_font_weight,
                italic: typography.accompaniment_translation_font_italic,
            },
            phonetic_font_size: typography.phonetic_font_size * lyrics_layout_scale,
            phonetic_line_height: typography.phonetic_line_height * lyrics_layout_scale,
            phonetic_attrs: TextAttrs {
                weight: typography.phonetic_font_weight,
                italic: typography.phonetic_font_italic,
            },
            phonetic_gap: spacing.phonetic_gap.max(0.0),
            translation_gap: spacing.translation_gap.max(0.0),
            accompaniment_translation_gap: spacing.accompaniment_translation_gap.max(0.0),
            padding_x: spacing.horizontal_padding * lyrics_layout_scale,
            padding_y: spacing.line_padding * lyrics_layout_scale,
            content_top: chrome.content_top,
            content_bottom: chrome.content_bottom,
            content_left: chrome.content_left,
            content_right: chrome.content_right,
            lyrics_clip_left: chrome.lyrics_clip_left,
            lyrics_clip_right: chrome.lyrics_clip_right,
            // `padding_y` is added when the glyphs are drawn. Subtract the doubled
            // Wide padding here so the focused lyric's visible top-left, rather
            // than merely its padded line box, lands at absolute 0.4H.
            keep_alive: resolve_keep_alive(&chrome, spacing),
            text_color: style.text_color,
            show_translation: style.show_translation,
            show_phonetic: style.show_phonetic,
            use_blur_effect: style.blur.enabled,
            blur_delta: style.blur.delta.max(0.0),
            accompaniment_gap: spacing.accompaniment_gap,
            blur_sharp_radius_lines: style.blur.sharp_radius_lines.max(0.0),
            inactive_karaoke_alpha: style.focus.inactive_karaoke_alpha.clamp(0.0, 1.0),
            focus_dim_min_alpha: style.focus.dim_min_alpha.clamp(0.0, 1.0),
            focus_dim_falloff_ms: style.focus.dim_falloff_ms.max(1.0),
            breathing_dots: BreathingDotsConfig {
                number: dots.number.clamp(1, 8),
                size: dots.size.max(1.0),
                margin: dots.margin.max(0.0),
                enter_ms: dots.enter_ms.max(1.0),
                still_ms: dots.still_ms.max(0.0),
                dip_ms: dots.dip_ms.max(1.0),
                exit_ms: dots.exit_ms.max(1.0),
                color: dots.color,
            },
            scroll_params: ScrollParams {
                spring_stiffness: spring.stiffness.max(0.0),
                spring_damping: spring.damping.max(0.0),
                chain_coupling: spring.chain_coupling,
                distance_falloff: spring.distance_falloff,
                min_response: spring.min_response.clamp(0.01, 1.0),
                max_fling_velocity: manual.max_fling_velocity.max(0.0),
                deceleration_rate: manual.deceleration_rate.clamp(0.0, 1.0),
                overscroll_stiffness: manual.overscroll_stiffness.max(0.0),
                overscroll_damping: manual.overscroll_damping.max(0.0),
                rubber_band_limit: manual.rubber_band_limit.max(1.0),
                rubber_band_coefficient: manual.rubber_band_coefficient.max(0.0001),
                blur_restore_ms: manual.blur_restore_ms,
                blur_fade_in_rate: manual.blur_fade_in_rate.max(0.0),
                blur_fade_out_rate: manual.blur_fade_out_rate.max(0.0),
            },
        };

        let content_width = (config.width as f32
            - config.content_left
            - config.content_right
            - config.padding_x * 2.0)
            .max(1.0);
        // Duet songs (lines aligned to both sides) lay each line out in an 80%-wide
        // band so the two singers' lines sit on opposite sides with an overlap in
        // the middle, instead of both spanning the full width. Right-aligned lines
        // are then shifted right by the freed 20% (`right_align_offset`) so they
        // still hug the true right edge. Solo songs (one alignment) keep full width.
        let is_duet = {
            let mut saw_left = false;
            let mut saw_right = false;
            for input in &scene.lines {
                if input_line_right_aligned(input) {
                    saw_right = true;
                } else {
                    saw_left = true;
                }
            }
            saw_left && saw_right
        };
        let layout_width = if is_duet {
            (content_width * DUET_LINE_WIDTH_RATIO).max(1.0)
        } else {
            content_width
        };
        let right_align_offset = content_width - layout_width;
        // An interlude belongs visually to the lyrics that follow it. Flattened
        // karaoke clusters may put a "before" accompaniment ahead of their main
        // line, so using either the immediately previous or current flattened row
        // can pick the wrong singer side. Resolve the next non-accompaniment line
        // up front and let every intervening row inherit that main line's side.
        let interlude_right_alignments = interlude_right_alignments(&scene.lines);
        let mut lines = Vec::with_capacity(scene.lines.len());
        let mut cursor_y = config.keep_alive;
        let mut previous_end: Option<i32> = None;

        for (line_index, input) in scene.lines.into_iter().enumerate() {
            let mut prepared = match input {
                LyricsLineInput::Karaoke(line) => {
                    let source_index = line.source_index.unwrap_or(line_index);
                    let cluster_index = line.cluster_index.unwrap_or(source_index);
                    let cluster_role = line
                        .cluster_role
                        .map(ClusterRole::from)
                        .unwrap_or(ClusterRole::Standalone);
                    let font_size = if line.is_accompaniment {
                        config.accompaniment_font_size
                    } else {
                        config.normal_font_size
                    };
                    let line_height = if line.is_accompaniment {
                        config.accompaniment_line_height
                    } else {
                        config.normal_line_height
                    };
                    // Physical alignment intentionally stays opposite to bidi's
                    // logical start for RTL text. Syllables keep their input order;
                    // each syllable's glyph shaping is still handled by cosmic-text.
                    let right_aligned = matches!(line.alignment, AlignmentInput::End);
                    let mut prepared_syllables =
                        prepare_karaoke_syllables(&line.syllables, line.is_accompaniment);
                    self.text_attrs = if line.is_accompaniment {
                        config.accompaniment_attrs
                    } else {
                        config.normal_attrs
                    };
                    self.phonetic_attrs = config.phonetic_attrs;
                    let prepared_text = self.prepare_karaoke_text_layout(
                        &line.syllables,
                        &mut prepared_syllables,
                        font_size,
                        line_height,
                        layout_width,
                        right_aligned,
                        config.show_phonetic,
                        config.phonetic_font_size,
                        config.phonetic_line_height,
                        config.phonetic_gap,
                    );
                    // An accompaniment line renders its translation at its own font
                    // size/attrs and its own body-to-translation gap; a main line
                    // uses the primary translation role and gap.
                    let (
                        translation_font_size,
                        translation_line_height,
                        translation_attrs,
                        detail_gap,
                    ) = if line.is_accompaniment {
                        (
                            config.accompaniment_translation_font_size,
                            config.accompaniment_translation_line_height,
                            config.accompaniment_translation_attrs,
                            config.accompaniment_translation_gap,
                        )
                    } else {
                        (
                            config.translation_font_size,
                            config.translation_line_height,
                            config.translation_attrs,
                            config.translation_gap,
                        )
                    };
                    let translation = if config.show_translation {
                        self.text_attrs = translation_attrs;
                        line.translation.as_deref().and_then(|translation| {
                            self.prepare_detail_text(
                                translation,
                                translation_font_size,
                                translation_line_height,
                                layout_width,
                                right_aligned,
                            )
                        })
                    } else {
                        None
                    };
                    let phonetic = if config.show_phonetic {
                        self.text_attrs = config.phonetic_attrs;
                        line.phonetic.as_deref().and_then(|phonetic| {
                            self.prepare_detail_text(
                                phonetic,
                                config.phonetic_font_size,
                                config.phonetic_line_height,
                                layout_width,
                                right_aligned,
                            )
                        })
                    } else {
                        None
                    };
                    let mut height = prepared_text.height + config.padding_y * 2.0;
                    if let Some(translation) = &translation {
                        height += translation.height + detail_gap;
                    }
                    if let Some(phonetic) = &phonetic {
                        height += phonetic.height + detail_gap;
                    }
                    PreparedLine {
                        source_index,
                        cluster_index,
                        cluster_role,
                        start: line.start,
                        end: line.end,
                        effective_end: line.end,
                        entrance_start: line.start,
                        height,
                        right_aligned,
                        x_offset: if right_aligned {
                            right_align_offset
                        } else {
                            0.0
                        },
                        interlude: None,
                        kind: PreparedLineKind::Karaoke {
                            is_accompaniment: line.is_accompaniment,
                            // RTL reverses the old sweep, while LTR remains LTR.
                            gradient_is_rtl: false,
                            syllables: prepared_syllables,
                            text: prepared_text,
                        },
                        translation,
                        phonetic,
                    }
                }
                LyricsLineInput::Synced(line) => {
                    let source_index = line.source_index.unwrap_or(line_index);
                    let cluster_index = line.cluster_index.unwrap_or(source_index);
                    let cluster_role = line
                        .cluster_role
                        .map(ClusterRole::from)
                        .unwrap_or(ClusterRole::Standalone);
                    self.text_attrs = config.normal_attrs;
                    let right_aligned = false;
                    let text = self.prepare_plain_text(
                        &line.content,
                        config.normal_font_size,
                        config.normal_line_height,
                        layout_width,
                        right_aligned,
                    );
                    let translation = if config.show_translation {
                        self.text_attrs = config.translation_attrs;
                        line.translation.as_deref().and_then(|translation| {
                            self.prepare_detail_text(
                                translation,
                                config.translation_font_size,
                                config.translation_line_height,
                                layout_width,
                                right_aligned,
                            )
                        })
                    } else {
                        None
                    };
                    let mut height = text.height + config.padding_y * 2.0;
                    if let Some(translation) = &translation {
                        height += translation.height + config.translation_gap;
                    }
                    PreparedLine {
                        source_index,
                        cluster_index,
                        cluster_role,
                        start: line.start,
                        end: line.end,
                        effective_end: line.end,
                        entrance_start: line.start,
                        height,
                        right_aligned,
                        x_offset: 0.0,
                        interlude: None,
                        kind: PreparedLineKind::Synced { text },
                        translation,
                        phonetic: None,
                    }
                }
            };

            if let Some(interlude) = make_interlude_slot(
                line_index,
                prepared.start,
                previous_end,
                interlude_right_alignments[line_index],
                &config,
            ) {
                prepared.height += interlude.height;
                prepared.interlude = Some(interlude);
            }

            cursor_y += prepared.height;
            previous_end = Some(prepared.end);
            lines.push(prepared);
        }

        // Anchor each nested accompaniment's appearance animation to its MAIN
        // line's start, so a main line and all of its accompaniment bloom in
        // together when the main appears (rather than each accompaniment blooming
        // only when it is itself first sung). Captured here — keyed by the original
        // cluster index, before the split below gives accompaniment lines their own.
        let mut cluster_main_start = HashMap::<usize, i32>::new();
        for line in &lines {
            if line.cluster_role == ClusterRole::Main {
                cluster_main_start.insert(line.cluster_index, line.start);
            }
        }
        for line in &mut lines {
            if line.cluster_role.is_nested_accompaniment() {
                if let Some(&main_start) = cluster_main_start.get(&line.cluster_index) {
                    line.entrance_start = main_start;
                }
            }
        }

        // Split oversized scroll clusters. A main line and its nested accompaniment
        // lines normally scroll and anchor as ONE block (a shared `cluster_index`),
        // but when their combined wrapped-row count is large, pinning the block's
        // top drags the sung line well below the focus. Break such a cluster into
        // per-line scroll units: each accompaniment keeps its `cluster_role` (so the
        // entrance bloom still fires) but gets its own fresh `cluster_index`, while
        // the main line stays on the original one — so the scroll anchors each part
        // individually instead of as one tall block. Runs before the effective-end
        // pass below so a split main line only stays focused for its own span. Uses
        // the same row cap as `focus_group_range` so the two agree on "oversized".
        let mut rows_by_cluster = HashMap::<usize, usize>::new();
        for line in &lines {
            *rows_by_cluster.entry(line.cluster_index).or_insert(0) += line.text_row_count();
        }
        let mut next_cluster_index = lines
            .iter()
            .map(|line| line.cluster_index)
            .max()
            .map(|max| max + 1)
            .unwrap_or(0);
        for line in &mut lines {
            let oversized = rows_by_cluster
                .get(&line.cluster_index)
                .is_some_and(|rows| *rows > MAX_SCROLL_GROUP_ROWS);
            if oversized && line.cluster_role.is_nested_accompaniment() {
                line.cluster_index = next_cluster_index;
                next_cluster_index += 1;
            }
        }

        let mut cluster_end_times = HashMap::<usize, i32>::new();
        for line in &lines {
            cluster_end_times
                .entry(line.cluster_index)
                .and_modify(|end| *end = (*end).max(line.end))
                .or_insert(line.end);
        }
        for line in &mut lines {
            if line.cluster_role == ClusterRole::Main {
                if let Some(end) = cluster_end_times.get(&line.cluster_index) {
                    line.effective_end = *end;
                }
            }
        }

        let top_bar = self.prepare_top_bar(scene.top_bar.as_ref(), chrome.thumb_border_width);

        Ok(PreparedScene {
            config,
            lines,
            content_height: cursor_y + config.keep_alive,
            top_bar,
        })
    }

    /// Shape the player top bar's title/artist once (with the engine's CJK-capable
    /// font fallback), using either the host's portrait geometry or the renderer's
    /// resolved landscape geometry. Text is single-line and clipped at draw time.
    fn prepare_top_bar(
        &mut self,
        input: Option<&TopBarInput>,
        thumb_border_width: f32,
    ) -> Option<PreparedTopBar> {
        let input = input?;
        let no_wrap = (input.text_max_width.max(1.0)) * 100.0;
        let saved = self.text_attrs;

        self.text_attrs = TextAttrs {
            weight: input.title_weight,
            italic: false,
        };
        let title = self.prepare_plain_text(
            &input.title,
            input.title_font_size,
            input.title_line_height,
            no_wrap,
            false,
        );
        self.text_attrs = TextAttrs {
            weight: 400,
            italic: false,
        };
        let artist = self.prepare_plain_text(
            &input.artist,
            input.artist_font_size,
            input.artist_line_height,
            no_wrap,
            false,
        );
        self.text_attrs = saved;

        let thumb_rect = skia_safe::Rect::new(
            input.thumb_left,
            input.thumb_top,
            input.thumb_left + input.thumb_size,
            input.thumb_top + input.thumb_size,
        );
        Some(PreparedTopBar {
            thumb_left: input.thumb_left,
            thumb_top: input.thumb_top,
            thumb_size: input.thumb_size,
            thumb_clip: crate::capsule::continuous_rounded_rect(thumb_rect, input.thumb_radius),
            thumb_border_width,
            text_left: input.text_left,
            text_max_width: input.text_max_width,
            title_top: input.title_top,
            artist_top: input.artist_top,
            artist_alpha: input.artist_alpha.clamp(0.0, 1.0),
            button_cx: input.button_cx,
            button_cy: input.button_cy,
            button_radius: input.button_radius,
            title,
            artist,
        })
    }

    pub(super) fn prepare_detail_text(
        &mut self,
        text: &str,
        font_size: f32,
        line_height: f32,
        width: f32,
        right_aligned: bool,
    ) -> Option<PreparedText> {
        if text.trim().is_empty() {
            return None;
        }
        Some(self.prepare_plain_text(text, font_size, line_height, width, right_aligned))
    }

    pub(super) fn prepare_plain_text(
        &mut self,
        text: &str,
        font_size: f32,
        line_height: f32,
        width: f32,
        right_aligned: bool,
    ) -> PreparedText {
        self.prepare_text_with_metadata(
            std::iter::once((text, 0usize)),
            text,
            font_size,
            line_height,
            width,
            right_aligned,
        )
    }

    pub(super) fn prepare_karaoke_text_layout(
        &mut self,
        input: &[SyllableInput],
        syllables: &mut [PreparedSyllable],
        font_size: f32,
        line_height: f32,
        width: f32,
        right_aligned: bool,
        show_phonetic: bool,
        phonetic_font_size: f32,
        phonetic_line_height: f32,
        phonetic_gap: f32,
    ) -> PreparedText {
        if input.is_empty() {
            return PreparedText {
                rows: Vec::new(),
                height: line_height,
                first_baseline: line_height,
            };
        }

        let space_width = self.measure_karaoke_space_width(font_size, line_height);
        let measured = input
            .iter()
            .enumerate()
            .map(|(index, syllable)| {
                let word_id = syllables
                    .get(index)
                    .map(|item| item.word_id)
                    .unwrap_or(index);
                let use_awesome = syllables
                    .get(index)
                    .map(|item| item.use_awesome)
                    .unwrap_or(false);
                self.measure_karaoke_syllable(
                    index,
                    word_id,
                    use_awesome,
                    &syllable.content,
                    syllable.phonetic.as_deref(),
                    font_size,
                    line_height,
                    show_phonetic,
                    phonetic_font_size,
                    phonetic_line_height,
                    space_width,
                )
            })
            .collect::<Vec<_>>();
        let measured = self.split_oversized_karaoke_syllables(
            measured,
            width,
            font_size,
            line_height,
            phonetic_font_size,
            phonetic_line_height,
            space_width,
        );

        let wrapped = self.calculate_balanced_lines(&measured, width, font_size, line_height);
        self.position_karaoke_wrapped_lines(
            wrapped,
            syllables,
            width,
            line_height,
            phonetic_line_height,
            phonetic_gap,
            right_aligned,
        )
    }

    pub(super) fn measure_karaoke_space_width(&mut self, font_size: f32, line_height: f32) -> f32 {
        let text = self.prepare_text_with_metadata(
            std::iter::once((" ", 0usize)),
            " ",
            font_size,
            line_height,
            font_size.max(1.0) * 4.0,
            false,
        );
        prepared_text_width(&text).max(font_size * 0.25)
    }

    pub(super) fn measure_karaoke_syllable(
        &mut self,
        index: usize,
        word_id: usize,
        use_awesome: bool,
        content: &str,
        phonetic: Option<&str>,
        font_size: f32,
        line_height: f32,
        show_phonetic: bool,
        phonetic_font_size: f32,
        phonetic_line_height: f32,
        space_width: f32,
    ) -> MeasuredSyllable {
        let text = self.prepare_single_syllable_text(content, index, font_size, line_height);
        let mut width = prepared_text_width(&text);
        let phonetic_text = if show_phonetic {
            phonetic
                .filter(|value| !value.trim().is_empty())
                .map(|value| {
                    // Phonetic annotations use their own weight/italic; restore the
                    // main syllable attrs afterwards for the rest of the measure.
                    let main_attrs = self.text_attrs;
                    self.text_attrs = self.phonetic_attrs;
                    let prepared = self.prepare_text_with_metadata(
                        std::iter::once((value, index + 1)),
                        value,
                        phonetic_font_size,
                        phonetic_line_height,
                        1_000_000.0,
                        false,
                    );
                    self.text_attrs = main_attrs;
                    prepared
                })
        } else {
            None
        };

        let trailing_spaces = trailing_whitespace_count(content);
        if trailing_spaces > 0 {
            let trimmed = trim_end_whitespace(content);
            let trimmed_width = if trimmed.is_empty() {
                0.0
            } else {
                let trimmed_text =
                    self.prepare_single_syllable_text(trimmed, index, font_size, line_height);
                prepared_text_width(&trimmed_text)
            };
            if width <= trimmed_width + 0.5 {
                width = trimmed_width + space_width * trailing_spaces as f32;
            }
        }
        if let Some(phonetic_text) = &phonetic_text {
            width = width.max(prepared_text_width(phonetic_text));
        }

        let draw_text = if use_awesome {
            self.prepare_awesome_syllable_text(content, index, font_size, line_height)
        } else {
            text.clone()
        };

        MeasuredSyllable {
            index,
            word_id,
            content: content.to_string(),
            use_awesome,
            allow_break_before: false,
            char_offset_in_syllable: 0,
            first_baseline: text.first_baseline,
            height: text.height,
            text: draw_text,
            phonetic: phonetic_text,
            width,
        }
    }

    /// Break a syllable that is wider than the lyric column into render-only
    /// fragments. Every fragment keeps the original syllable index/word id, so
    /// timing and karaoke animation still come from the source syllable. We prefer
    /// whitespace/punctuation boundaries and fall back to Unicode grapheme
    /// boundaries for languages that do not use spaces.
    pub(super) fn split_oversized_karaoke_syllables(
        &mut self,
        measured: Vec<MeasuredSyllable>,
        available_width: f32,
        font_size: f32,
        line_height: f32,
        phonetic_font_size: f32,
        phonetic_line_height: f32,
        space_width: f32,
    ) -> Vec<MeasuredSyllable> {
        let mut result = Vec::with_capacity(measured.len());
        for layout in measured {
            let grapheme_boundaries = layout
                .content
                .grapheme_indices(true)
                .map(|(offset, grapheme)| offset + grapheme.len())
                .collect::<Vec<_>>();
            if layout.width <= available_width
                || grapheme_boundaries.len() <= 1
                // A phonetic annotation belongs to the complete syllable. Until
                // annotations themselves have row-aware layout, keep that pair
                // intact rather than duplicating or dropping the annotation.
                || layout.phonetic.is_some()
            {
                result.push(layout);
                continue;
            }

            let mut start = 0usize;
            let mut first_fragment = true;
            while start < layout.content.len() {
                let mut last_fit = None;
                let mut preferred_fit = None;

                for &end in grapheme_boundaries.iter().filter(|&&end| end > start) {
                    let content = &layout.content[start..end];
                    let candidate = self.measure_karaoke_syllable(
                        layout.index,
                        layout.word_id,
                        false,
                        content,
                        None,
                        font_size,
                        line_height,
                        false,
                        phonetic_font_size,
                        phonetic_line_height,
                        space_width,
                    );
                    if candidate.width <= available_width {
                        last_fit = Some(end);
                        if content.chars().next_back().is_some_and(|ch| {
                            ch.is_whitespace() || is_punctuation_or_space(&ch.to_string())
                        }) {
                            preferred_fit = Some(end);
                        }
                    } else {
                        // A single grapheme can itself be wider than the viewport;
                        // it is the smallest safe unit and must remain intact.
                        if last_fit.is_none() {
                            last_fit = Some(end);
                        }
                        break;
                    }
                }

                let chosen_end = if last_fit == Some(layout.content.len()) {
                    last_fit
                } else {
                    preferred_fit.or(last_fit)
                };
                let Some(end) = chosen_end else {
                    break;
                };
                let content = &layout.content[start..end];
                let mut fragment = self.measure_karaoke_syllable(
                    layout.index,
                    layout.word_id,
                    layout.use_awesome,
                    content,
                    None,
                    font_size,
                    line_height,
                    false,
                    phonetic_font_size,
                    phonetic_line_height,
                    space_width,
                );
                fragment.allow_break_before = !first_fragment;
                fragment.char_offset_in_syllable = layout.content[..start].chars().count();
                result.push(fragment);
                first_fragment = false;
                start = end;
            }
        }
        result
    }

    pub(super) fn prepare_awesome_syllable_text(
        &mut self,
        content: &str,
        index: usize,
        font_size: f32,
        line_height: f32,
    ) -> PreparedText {
        let mut glyphs = Vec::new();
        let mut x_offset = 0.0f32;
        let mut first_baseline = None;

        for (char_index, ch) in content.chars().enumerate() {
            let char_text = ch.to_string();
            let measured =
                self.prepare_single_syllable_text(&char_text, index, font_size, line_height);
            first_baseline.get_or_insert(measured.first_baseline);
            for source_row in &measured.rows {
                for source_glyph in &source_row.glyphs {
                    let mut glyph = source_glyph.clone();
                    glyph.physical.x += x_offset.round() as i32;
                    glyph.x += x_offset;
                    glyph.glyph_index_in_syllable = char_index;
                    glyph.animation_char_index = char_index as f32;
                    glyphs.push(glyph);
                }
            }
            x_offset += prepared_text_width(&measured);
        }

        PreparedText {
            rows: vec![PreparedRow {
                y: 0.0,
                width: x_offset,
                min_x: 0.0,
                max_x: x_offset,
                glyphs,
                syllable_segments: Vec::new(),
            }],
            height: line_height,
            first_baseline: first_baseline.unwrap_or(line_height),
        }
    }

    pub(super) fn prepare_single_syllable_text(
        &mut self,
        content: &str,
        index: usize,
        font_size: f32,
        line_height: f32,
    ) -> PreparedText {
        self.prepare_text_with_metadata(
            std::iter::once((content, index + 1)),
            content,
            font_size,
            line_height,
            1_000_000.0,
            false,
        )
    }

    pub(super) fn calculate_balanced_lines(
        &mut self,
        syllable_layouts: &[MeasuredSyllable],
        available_width: f32,
        font_size: f32,
        line_height: f32,
    ) -> Vec<WrappedMeasuredLine> {
        if syllable_layouts.is_empty() {
            return Vec::new();
        }

        let n = syllable_layouts.len();
        let mut costs = vec![f64::INFINITY; n + 1];
        let mut breaks = vec![0usize; n + 1];
        costs[0] = 0.0;

        for i in 1..=n {
            let mut current_line_width = 0.0f32;
            for j in (1..=i).rev() {
                if j > 1
                    && syllable_layouts[j - 2].word_id == syllable_layouts[j - 1].word_id
                    && !syllable_layouts[j - 1].allow_break_before
                {
                    current_line_width += syllable_layouts[j - 1].width;
                    if current_line_width > available_width {
                        break;
                    }
                    continue;
                }

                current_line_width += syllable_layouts[j - 1].width;
                if current_line_width > available_width {
                    break;
                }

                let badness = (available_width - current_line_width).powi(2) as f64;
                if costs[j - 1].is_finite() && costs[j - 1] + badness < costs[i] {
                    costs[i] = costs[j - 1] + badness;
                    breaks[i] = j - 1;
                }
            }
        }

        if !costs[n].is_finite() {
            return self.calculate_greedy_wrapped_lines(
                syllable_layouts,
                available_width,
                font_size,
                line_height,
            );
        }

        let mut lines = Vec::new();
        let mut current_index = n;
        while current_index > 0 {
            let start_index = breaks[current_index];
            if start_index >= current_index {
                return self.calculate_greedy_wrapped_lines(
                    syllable_layouts,
                    available_width,
                    font_size,
                    line_height,
                );
            }

            let trimmed = self.trim_display_line_trailing_spaces(
                &syllable_layouts[start_index..current_index],
                font_size,
                line_height,
            );
            if !trimmed.syllables.is_empty() {
                lines.insert(0, trimmed);
            }
            current_index = start_index;
        }

        lines
    }

    pub(super) fn calculate_greedy_wrapped_lines(
        &mut self,
        syllable_layouts: &[MeasuredSyllable],
        available_width: f32,
        font_size: f32,
        line_height: f32,
    ) -> Vec<WrappedMeasuredLine> {
        let mut lines = Vec::new();
        let mut current_line = Vec::<MeasuredSyllable>::new();
        let mut current_line_width = 0.0f32;

        let mut word_groups = Vec::<Vec<MeasuredSyllable>>::new();
        if let Some(first) = syllable_layouts.first() {
            let mut current_word_id = first.word_id;
            let mut current_word_group = Vec::new();
            for layout in syllable_layouts {
                if layout.word_id != current_word_id || layout.allow_break_before {
                    word_groups.push(current_word_group);
                    current_word_group = Vec::new();
                    current_word_id = layout.word_id;
                }
                current_word_group.push(layout.clone());
            }
            word_groups.push(current_word_group);
        }

        for word_syllables in word_groups {
            let word_width = word_syllables
                .iter()
                .map(|layout| layout.width)
                .sum::<f32>();

            if current_line_width + word_width <= available_width {
                current_line_width += word_width;
                current_line.extend(word_syllables);
                continue;
            }

            if !current_line.is_empty() {
                let trimmed =
                    self.trim_display_line_trailing_spaces(&current_line, font_size, line_height);
                if !trimmed.syllables.is_empty() {
                    lines.push(trimmed);
                }
                current_line.clear();
                current_line_width = 0.0;
            }

            if word_width <= available_width {
                current_line_width += word_width;
                current_line.extend(word_syllables);
            } else {
                for syllable in word_syllables {
                    if current_line_width + syllable.width > available_width
                        && !current_line.is_empty()
                    {
                        let trimmed = self.trim_display_line_trailing_spaces(
                            &current_line,
                            font_size,
                            line_height,
                        );
                        if !trimmed.syllables.is_empty() {
                            lines.push(trimmed);
                        }
                        current_line.clear();
                        current_line_width = 0.0;
                    }
                    current_line_width += syllable.width;
                    current_line.push(syllable);
                }
            }
        }

        if !current_line.is_empty() {
            let trimmed =
                self.trim_display_line_trailing_spaces(&current_line, font_size, line_height);
            if !trimmed.syllables.is_empty() {
                lines.push(trimmed);
            }
        }

        lines
    }

    pub(super) fn trim_display_line_trailing_spaces(
        &mut self,
        display_line_syllables: &[MeasuredSyllable],
        font_size: f32,
        line_height: f32,
    ) -> WrappedMeasuredLine {
        if display_line_syllables.is_empty() {
            return WrappedMeasuredLine {
                syllables: Vec::new(),
                total_width: 0.0,
            };
        }

        let mut processed = display_line_syllables.to_vec();
        while processed
            .last()
            .is_some_and(|layout| is_blank_text(&layout.content))
        {
            processed.pop();
        }

        if processed.is_empty() {
            return WrappedMeasuredLine {
                syllables: Vec::new(),
                total_width: 0.0,
            };
        }

        let last_index = processed.len() - 1;
        let original_content = processed[last_index].content.clone();
        let trimmed_content = trim_end_whitespace(&original_content);
        if trimmed_content.len() < original_content.len() {
            if trimmed_content.is_empty() {
                processed.pop();
            } else {
                let index = processed[last_index].index;
                let text = self.prepare_single_syllable_text(
                    trimmed_content,
                    index,
                    font_size,
                    line_height,
                );
                let draw_text = if processed[last_index].use_awesome {
                    self.prepare_awesome_syllable_text(
                        trimmed_content,
                        index,
                        font_size,
                        line_height,
                    )
                } else {
                    text.clone()
                };
                let phonetic_width = processed[last_index]
                    .phonetic
                    .as_ref()
                    .map(prepared_text_width)
                    .unwrap_or(0.0);
                let mut replacement = processed[last_index].clone();
                replacement.content = trimmed_content.to_string();
                replacement.first_baseline = text.first_baseline;
                replacement.height = text.height;
                replacement.text = draw_text;
                replacement.width = prepared_text_width(&text).max(phonetic_width);
                processed[last_index] = replacement;
            }
        }

        let total_width = processed.iter().map(|layout| layout.width).sum::<f32>();
        WrappedMeasuredLine {
            syllables: processed,
            total_width,
        }
    }

    pub(super) fn position_karaoke_wrapped_lines(
        &self,
        wrapped_lines: Vec<WrappedMeasuredLine>,
        syllables: &mut [PreparedSyllable],
        canvas_width: f32,
        line_height: f32,
        phonetic_line_height: f32,
        phonetic_gap: f32,
        right_aligned: bool,
    ) -> PreparedText {
        let mut rows = Vec::new();
        let mut first_baseline = None;
        let mut bounds_by_word = HashMap::<usize, (f32, f32, f32)>::new();
        let mut total_width_by_syllable = HashMap::<usize, f32>::new();
        for line in &wrapped_lines {
            for layout in &line.syllables {
                *total_width_by_syllable.entry(layout.index).or_insert(0.0) += layout.width;
            }
        }
        let mut positioned_width_by_syllable = HashMap::<usize, f32>::new();
        let has_phonetic_in_block = wrapped_lines.iter().any(|line| {
            line.syllables
                .iter()
                .any(|layout| layout.phonetic.is_some())
        });
        let row_height = line_height
            + if has_phonetic_in_block {
                phonetic_line_height * 0.7
            } else {
                0.0
            };
        let first_row_offset = if has_phonetic_in_block {
            phonetic_line_height * 0.7
        } else {
            0.0
        };

        for (line_index, wrapped_line) in wrapped_lines.into_iter().enumerate() {
            if wrapped_line.syllables.is_empty() {
                continue;
            }

            let max_baseline = wrapped_line
                .syllables
                .iter()
                .map(|layout| layout.first_baseline)
                .fold(0.0, f32::max);
            first_baseline.get_or_insert(max_baseline);

            let row_top_y = line_index as f32 * row_height + first_row_offset;
            let start_x = if right_aligned {
                canvas_width - wrapped_line.total_width
            } else {
                0.0
            };
            let mut current_x = start_x;
            let mut row_glyphs = Vec::new();
            let mut syllable_segments = Vec::<PreparedSyllableSegment>::new();

            for layout in wrapped_line.syllables {
                let position_x = current_x;
                let vertical_offset = max_baseline - layout.first_baseline;
                let position_y = row_top_y + vertical_offset;
                let bottom_y = position_y + layout.height;
                if let Some(syllable) = syllables.get_mut(layout.index) {
                    syllable.layout_x = position_x;
                    syllable.layout_width = layout.width;
                }
                let segment_max_x = position_x + layout.width;
                let total_syllable_width = total_width_by_syllable
                    .get(&layout.index)
                    .copied()
                    .unwrap_or(layout.width)
                    .max(1.0);
                let positioned_width = positioned_width_by_syllable
                    .entry(layout.index)
                    .or_insert(0.0);
                let progress_start = (*positioned_width / total_syllable_width).clamp(0.0, 1.0);
                *positioned_width += layout.width;
                let progress_end = (*positioned_width / total_syllable_width).clamp(0.0, 1.0);
                if let Some(segment) = syllable_segments.last_mut().filter(|segment| {
                    segment.syllable_index == layout.index
                        && (segment.max_x - position_x).abs() < 0.5
                }) {
                    segment.max_x = segment_max_x;
                    segment.progress_end = progress_end;
                } else {
                    syllable_segments.push(PreparedSyllableSegment {
                        syllable_index: layout.index,
                        min_x: position_x,
                        max_x: segment_max_x,
                        progress_start,
                        progress_end,
                    });
                }

                bounds_by_word
                    .entry(layout.word_id)
                    .and_modify(|bounds| {
                        bounds.0 = bounds.0.min(position_x);
                        bounds.1 = bounds.1.max(position_x + layout.width);
                        bounds.2 = bounds.2.max(bottom_y);
                    })
                    .or_insert((position_x, position_x + layout.width, bottom_y));

                let shift_x = position_x.round() as i32;
                let shift_y = position_y.round() as i32;
                for source_row in &layout.text.rows {
                    for source_glyph in &source_row.glyphs {
                        let mut glyph = source_glyph.clone();
                        glyph.physical.x += shift_x;
                        glyph.physical.y += shift_y;
                        glyph.x += position_x;
                        glyph.animation_char_index += layout.char_offset_in_syllable as f32;
                        row_glyphs.push(glyph);
                    }
                }
                if let Some(phonetic_text) = &layout.phonetic {
                    let phonetic_y = position_y - phonetic_text.height + phonetic_gap;
                    let phonetic_shift_x = position_x.round() as i32;
                    let phonetic_shift_y = phonetic_y.round() as i32;
                    let phonetic_animation_index =
                        (layout.content.chars().count().saturating_sub(1) as f32) * 0.5;
                    for source_row in &phonetic_text.rows {
                        for source_glyph in &source_row.glyphs {
                            let mut glyph = source_glyph.clone();
                            glyph.physical.x += phonetic_shift_x;
                            glyph.physical.y += phonetic_shift_y;
                            glyph.x += position_x;
                            glyph.animation_char_index = phonetic_animation_index;
                            glyph.alpha_multiplier = 0.4;
                            glyph.is_phonetic = true;
                            row_glyphs.push(glyph);
                        }
                    }
                }

                current_x += layout.width;
            }

            rows.push(PreparedRow {
                y: row_top_y,
                width: wrapped_line.total_width,
                min_x: start_x,
                max_x: start_x + wrapped_line.total_width,
                glyphs: row_glyphs,
                syllable_segments,
            });
        }

        for syllable in syllables {
            if let Some((min_x, max_x, bottom_y)) = bounds_by_word.get(&syllable.word_id) {
                syllable.word_pivot_x = (min_x + max_x) * 0.5;
                syllable.word_pivot_y = *bottom_y;
            }
        }

        let row_count = rows.len().max(1);
        PreparedText {
            rows,
            height: row_count as f32
                * (line_height
                    + if has_phonetic_in_block {
                        phonetic_line_height
                    } else {
                        0.0
                    }),
            first_baseline: first_baseline.unwrap_or(line_height),
        }
    }

    pub(super) fn prepare_text_with_metadata<'a>(
        &mut self,
        spans: impl Iterator<Item = (&'a str, usize)>,
        fallback_text: &str,
        font_size: f32,
        line_height: f32,
        width: f32,
        right_aligned: bool,
    ) -> PreparedText {
        let metrics = Metrics::new(font_size, line_height);
        let text_attrs = self.text_attrs;
        // Lazily load any system fonts this text needs and register them in the
        // fallback pool BEFORE choosing per-cluster families — otherwise selection
        // runs against the user chain only, locks in a family that can't render
        // CJK/symbols, and the matched system font (e.g. MiSans) never gets used.
        let spans: Vec<(&str, usize)> = spans.collect();
        #[cfg(target_os = "android")]
        {
            for (text, _) in &spans {
                self.ensure_fonts_for_text(text, text_attrs);
            }
            if spans.is_empty() {
                self.ensure_fonts_for_text(fallback_text, text_attrs);
            }
        }
        let font_spans = self.build_font_spans(spans.iter().copied(), fallback_text);
        let first_family_name = self.font_stack.first().map(|face| face.family_name.clone());
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        {
            let mut borrowed = buffer.borrow_with(&mut self.font_system);
            borrowed.set_wrap(Wrap::WordOrGlyph);
            borrowed.set_size(Some(width.max(1.0)), None);
            // Weight/italic come from the role being shaped (set in prepare_scene);
            // the per-cluster family is still chosen below. Size is independent.
            let default_attrs = Attrs::new()
                .weight(text_attrs.cosmic_weight())
                .style(text_attrs.cosmic_style());
            let rich_spans = font_spans
                .iter()
                .map(|span| {
                    let attrs = match span.family_name.as_deref() {
                        Some(family_name) => default_attrs
                            .clone()
                            .family(Family::Name(family_name))
                            .metadata(span.metadata),
                        None => default_attrs.clone().metadata(span.metadata),
                    };
                    (span.text.as_str(), attrs)
                })
                .collect::<Vec<_>>();
            if rich_spans.is_empty() {
                let fallback_attrs = match first_family_name.as_deref() {
                    Some(family_name) => default_attrs
                        .clone()
                        .family(Family::Name(family_name))
                        .metadata(0),
                    None => default_attrs.clone().metadata(0),
                };
                borrowed.set_text(
                    fallback_text,
                    &fallback_attrs,
                    Shaping::Advanced,
                    Some(if right_aligned {
                        Align::Right
                    } else {
                        Align::Left
                    }),
                );
            } else {
                borrowed.set_rich_text(
                    rich_spans
                        .iter()
                        .map(|(text, attrs)| (*text, attrs.clone())),
                    &default_attrs,
                    Shaping::Advanced,
                    Some(if right_aligned {
                        Align::Right
                    } else {
                        Align::Left
                    }),
                );
            }
            borrowed.shape_until_scroll(false);
        }

        let mut borrowed = buffer.borrow_with(&mut self.font_system);
        let mut rows = Vec::new();
        let mut first_baseline = None;
        let mut glyph_counters_by_syllable = HashMap::<usize, usize>::new();
        for run in borrowed.layout_runs() {
            first_baseline.get_or_insert(run.line_y - run.line_top);
            let mut glyphs = Vec::with_capacity(run.glyphs.len());
            for glyph in run.glyphs.iter() {
                let syllable_index = if glyph.metadata > 0 {
                    Some(glyph.metadata - 1)
                } else {
                    None
                };
                let glyph_index_in_syllable = syllable_index
                    .map(|index| {
                        let counter = glyph_counters_by_syllable.entry(index).or_insert(0);
                        let current = *counter;
                        *counter += 1;
                        current
                    })
                    .unwrap_or(0);
                glyphs.push(PreparedGlyph {
                    physical: glyph.physical((0.0, run.line_y), 1.0),
                    x: glyph.x,
                    syllable_index,
                    glyph_index_in_syllable,
                    animation_char_index: glyph_index_in_syllable as f32,
                    alpha_multiplier: 1.0,
                    is_phonetic: false,
                });
            }
            rows.push(PreparedRow {
                y: run.line_top,
                width: run.line_w,
                min_x: 0.0,
                max_x: run.line_w,
                glyphs,
                syllable_segments: Vec::new(),
            });
        }

        let height = rows
            .last()
            .map(|row| row.y + line_height)
            .unwrap_or(line_height);

        PreparedText {
            rows,
            height,
            first_baseline: first_baseline.unwrap_or(line_height),
        }
    }
}

/// Whether an input line lays out right-aligned — mirrors the per-line logic in
/// `prepare_scene`, used up front to decide if the song is a duet (has lines on
/// both sides).
fn input_line_right_aligned(input: &LyricsLineInput) -> bool {
    match input {
        LyricsLineInput::Karaoke(line) => matches!(line.alignment, AlignmentInput::End),
        LyricsLineInput::Synced(_) => false,
    }
}

/// Resolve the side of the next main (non-accompaniment) line for every flattened
/// input row. A trailing accompaniment with no following main falls back to its
/// own side, which keeps malformed/incomplete input deterministic.
pub(super) fn interlude_right_alignments(inputs: &[LyricsLineInput]) -> Vec<bool> {
    let mut result = vec![false; inputs.len()];
    let mut next_main_alignment = None;

    for (index, input) in inputs.iter().enumerate().rev() {
        let is_main = match input {
            LyricsLineInput::Karaoke(line) => !line.is_accompaniment,
            LyricsLineInput::Synced(_) => true,
        };
        if is_main {
            next_main_alignment = Some(input_line_right_aligned(input));
        }
        result[index] = next_main_alignment.unwrap_or_else(|| input_line_right_aligned(input));
    }

    result
}

pub(super) fn prepare_karaoke_syllables(
    input: &[SyllableInput],
    is_accompaniment: bool,
) -> Vec<PreparedSyllable> {
    let mut prepared = input
        .iter()
        .map(|syllable| PreparedSyllable {
            content: syllable.content.clone(),
            start: syllable.start,
            end: syllable.end,
            word_id: 0,
            float_end: syllable.end,
            use_awesome: false,
            char_count: syllable.content.chars().count(),
            char_offset_in_word: 0,
            word_start: syllable.start,
            word_end: syllable.end,
            word_duration: (syllable.end - syllable.start).max(1),
            word_char_count: syllable.content.chars().count().max(1),
            word_pivot_x: 0.0,
            word_pivot_y: 0.0,
            layout_x: 0.0,
            layout_width: 0.0,
        })
        .collect::<Vec<_>>();

    if prepared.is_empty() {
        return prepared;
    }

    let mut word_start_index = 0usize;
    let mut word_id_ranges = Vec::<std::ops::Range<usize>>::new();
    for index in 0..prepared.len() {
        if has_trailing_whitespace(&prepared[index].content) {
            word_id_ranges.push(word_start_index..index + 1);
            word_start_index = index + 1;
        }
    }
    if word_start_index < prepared.len() {
        word_id_ranges.push(word_start_index..prepared.len());
    }

    for (word_id, range) in word_id_ranges.into_iter().enumerate() {
        let word_start = prepared[range.start].start;
        let word_end = prepared[range.end - 1].end;
        let word_duration = (word_end - word_start).max(1);
        let word_content = prepared[range.clone()]
            .iter()
            .map(|syllable| syllable.content.as_str())
            .collect::<String>();
        let word_char_count = word_content.chars().count().max(1);
        let per_char_duration = word_duration as f32 / word_char_count as f32;
        let use_awesome = !is_accompaniment
            && word_duration >= AWESOME_MIN_WORD_DURATION_MS
            && per_char_duration > AWESOME_FAST_CHAR_THRESHOLD_MS
            && !should_use_simple_animation(&word_content);

        let mut char_offset = 0usize;
        for index in range {
            prepared[index].word_id = word_id;
            prepared[index].use_awesome = use_awesome;
            prepared[index].char_offset_in_word = char_offset;
            prepared[index].word_start = word_start;
            prepared[index].word_end = word_end;
            prepared[index].word_duration = word_duration;
            prepared[index].word_char_count = word_char_count;
            char_offset += prepared[index].char_count;
        }
    }

    let line_end = prepared.last().map(|syllable| syllable.end).unwrap_or(0);
    let mut float_ends = vec![line_end; prepared.len()];
    for index in 0..prepared.len() {
        let mut end = (prepared[index].start + 700).min(line_end);
        if index + 1 < prepared.len() && prepared[index + 1].use_awesome {
            end = end.min(prepared[index + 1].start);
        }
        float_ends[index] = end.max(prepared[index].start + 1);
    }

    for index in (0..prepared.len().saturating_sub(1)).rev() {
        if !prepared[index + 1].use_awesome && float_ends[index] > float_ends[index + 1] {
            float_ends[index] = float_ends[index + 1].max(prepared[index].start + 1);
        }
    }

    for (syllable, float_end) in prepared.iter_mut().zip(float_ends) {
        syllable.float_end = float_end;
    }

    prepared
}
