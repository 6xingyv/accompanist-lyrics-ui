//! Scene preparation & text layout: turning a `LyricsScene` into positioned,
//! shaped `PreparedScene` rows — karaoke syllable measurement, line balancing /
//! greedy wrapping, and per-cluster font-span shaping. Split out of `renderer.rs`.

use super::*;

impl LyricsRenderer {
    pub(super) fn prepare_scene(&mut self, scene: LyricsScene) -> Result<PreparedScene, String> {
        let locale = scene.locale.as_deref().unwrap_or("en-US");
        self.set_locale(locale);

        let config = SceneConfig {
            width: scene.width.unwrap_or(DEFAULT_WIDTH).max(DEFAULT_WIDTH),
            height: scene.height.unwrap_or(DEFAULT_HEIGHT).max(DEFAULT_HEIGHT),
            normal_font_size: scene.normal_font_size.unwrap_or(DEFAULT_NORMAL_FONT_SIZE),
            normal_line_height: scene
                .normal_line_height
                .unwrap_or(DEFAULT_NORMAL_LINE_HEIGHT),
            normal_attrs: TextAttrs {
                weight: scene.normal_font_weight.unwrap_or(400),
                italic: scene.normal_font_italic.unwrap_or(false),
            },
            accompaniment_font_size: scene
                .accompaniment_font_size
                .unwrap_or(DEFAULT_ACCOMPANIMENT_FONT_SIZE),
            accompaniment_line_height: scene
                .accompaniment_line_height
                .unwrap_or(DEFAULT_ACCOMPANIMENT_LINE_HEIGHT),
            accompaniment_attrs: TextAttrs {
                weight: scene.accompaniment_font_weight.unwrap_or(400),
                italic: scene.accompaniment_font_italic.unwrap_or(false),
            },
            translation_font_size: scene
                .translation_font_size
                .unwrap_or(DEFAULT_TRANSLATION_FONT_SIZE),
            translation_line_height: scene
                .translation_line_height
                .unwrap_or(DEFAULT_TRANSLATION_LINE_HEIGHT),
            translation_attrs: TextAttrs {
                weight: scene.translation_font_weight.unwrap_or(400),
                italic: scene.translation_font_italic.unwrap_or(false),
            },
            phonetic_font_size: scene.phonetic_font_size.unwrap_or_else(|| {
                scene
                    .translation_font_size
                    .unwrap_or(DEFAULT_TRANSLATION_FONT_SIZE)
            }),
            phonetic_line_height: scene.phonetic_line_height.unwrap_or_else(|| {
                scene
                    .translation_line_height
                    .unwrap_or(DEFAULT_TRANSLATION_LINE_HEIGHT)
            }),
            phonetic_attrs: TextAttrs {
                weight: scene.phonetic_font_weight.unwrap_or(400),
                italic: scene.phonetic_font_italic.unwrap_or(false),
            },
            phonetic_gap: scene.phonetic_gap.unwrap_or(4.0).max(0.0),
            padding_x: scene.padding_x.unwrap_or(DEFAULT_PADDING_X),
            padding_y: scene.padding_y.unwrap_or(DEFAULT_PADDING_Y),
            keep_alive: scene.keep_alive.unwrap_or(DEFAULT_KEEP_ALIVE),
            text_color: scene.text_color.unwrap_or(0xffff_ffff),
            show_translation: scene.show_translation.unwrap_or(true),
            show_phonetic: scene.show_phonetic.unwrap_or(true),
            use_blur_effect: scene.use_blur_effect.unwrap_or(true),
            blur_delta: scene.blur_delta.unwrap_or(3.0).max(0.0),
            accompaniment_gap: scene.accompaniment_gap.unwrap_or(0.0),
            blur_sharp_radius_lines: scene
                .blur_sharp_radius_lines
                .unwrap_or(BLUR_SHARP_RADIUS_LINES)
                .max(0.0),
            inactive_karaoke_alpha: scene
                .inactive_karaoke_alpha
                .unwrap_or(KARAOKE_INACTIVE_ALPHA)
                .clamp(0.0, 1.0),
            focus_dim_min_alpha: scene
                .focus_dim_min_alpha
                .unwrap_or(FOCUS_ALPHA_MIN)
                .clamp(0.0, 1.0),
            focus_dim_falloff_ms: scene.focus_dim_falloff_ms.unwrap_or(FOCUS_ALPHA_FALLOFF_MS).max(1.0),
            breathing_dots: BreathingDotsConfig {
                number: scene
                    .breathing_dots_number
                    .unwrap_or(DEFAULT_DOTS_NUMBER)
                    .clamp(1, 8),
                size: scene
                    .breathing_dots_size
                    .unwrap_or(DEFAULT_DOTS_SIZE)
                    .max(1.0),
                margin: scene
                    .breathing_dots_margin
                    .unwrap_or(DEFAULT_DOTS_MARGIN)
                    .max(0.0),
                enter_ms: scene
                    .breathing_dots_enter_ms
                    .unwrap_or(DEFAULT_DOTS_ENTER_MS)
                    .max(1.0),
                still_ms: scene
                    .breathing_dots_still_ms
                    .unwrap_or(DEFAULT_DOTS_STILL_MS)
                    .max(0.0),
                dip_ms: scene
                    .breathing_dots_dip_ms
                    .unwrap_or(DEFAULT_DOTS_DIP_MS)
                    .max(1.0),
                exit_ms: scene
                    .breathing_dots_exit_ms
                    .unwrap_or(DEFAULT_DOTS_EXIT_MS)
                    .max(1.0),
                color: scene
                    .breathing_dots_color
                    .unwrap_or_else(|| scene.text_color.unwrap_or(0xffff_ffff)),
            },
            scroll_params: ScrollParams {
                spring_stiffness: scene
                    .spring_stiffness
                    .unwrap_or(LINE_LAYOUT_SPRING_STIFFNESS)
                    .max(0.0),
                spring_damping: scene.spring_damping.unwrap_or(LINE_LAYOUT_SPRING_DAMPING).max(0.0),
                chain_coupling: scene.spring_chain_coupling.unwrap_or(LINE_LAYOUT_CHAIN_COUPLING),
                distance_falloff: scene
                    .spring_distance_falloff
                    .unwrap_or(LINE_LAYOUT_DISTANCE_FALLOFF),
                min_response: scene
                    .spring_min_response
                    .unwrap_or(LINE_LAYOUT_MIN_RESPONSE)
                    .clamp(0.01, 1.0),
                max_fling_velocity: scene
                    .manual_max_fling_velocity
                    .unwrap_or(MANUAL_SCROLL_MAX_FLING_VELOCITY)
                    .max(0.0),
                deceleration_rate: scene
                    .manual_deceleration_rate
                    .unwrap_or(MANUAL_SCROLL_DECELERATION_RATE)
                    .clamp(0.0, 1.0),
                overscroll_stiffness: scene
                    .manual_overscroll_stiffness
                    .unwrap_or(MANUAL_SCROLL_OVERSCROLL_STIFFNESS)
                    .max(0.0),
                overscroll_damping: scene
                    .manual_overscroll_damping
                    .unwrap_or(MANUAL_SCROLL_OVERSCROLL_DAMPING)
                    .max(0.0),
                rubber_band_limit: scene
                    .manual_rubber_band_limit
                    .unwrap_or(MANUAL_SCROLL_RUBBER_BAND_LIMIT)
                    .max(1.0),
                rubber_band_coefficient: scene
                    .manual_rubber_band_coefficient
                    .unwrap_or(MANUAL_SCROLL_RUBBER_BAND_COEFFICIENT)
                    .max(0.0001),
                blur_restore_ms: scene
                    .manual_blur_restore_ms
                    .unwrap_or(MANUAL_SCROLL_BLUR_RESTORE_MS),
                blur_fade_in_rate: scene
                    .manual_blur_fade_in_rate
                    .unwrap_or(MANUAL_SCROLL_BLUR_FADE_IN_RATE)
                    .max(0.0),
                blur_fade_out_rate: scene
                    .manual_blur_fade_out_rate
                    .unwrap_or(MANUAL_SCROLL_BLUR_FADE_OUT_RATE)
                    .max(0.0),
            },
        };

        let content_width = (config.width as f32 - config.padding_x * 2.0).max(1.0);
        let mut lines = Vec::with_capacity(scene.lines.len());
        let mut cursor_y = config.keep_alive;
        let mut previous_end: Option<i32> = None;
        let mut previous_right_aligned = false;

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
                    let is_rtl = line.syllables.iter().any(|s| contains_rtl(&s.content));
                    let right_aligned = match line.alignment {
                        AlignmentInput::Start | AlignmentInput::Unspecified => is_rtl,
                        AlignmentInput::End => !is_rtl,
                    };
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
                        content_width,
                        right_aligned,
                        is_rtl,
                        config.show_phonetic,
                        config.phonetic_font_size,
                        config.phonetic_line_height,
                        config.phonetic_gap,
                    );
                    let translation = if config.show_translation {
                        self.text_attrs = config.translation_attrs;
                        line.translation.as_deref().and_then(|translation| {
                            self.prepare_detail_text(
                                translation,
                                config.translation_font_size,
                                config.translation_line_height,
                                content_width,
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
                                content_width,
                                right_aligned,
                            )
                        })
                    } else {
                        None
                    };
                    let mut height = prepared_text.height + config.padding_y * 2.0;
                    if let Some(translation) = &translation {
                        height += translation.height + ROW_GAP;
                    }
                    if let Some(phonetic) = &phonetic {
                        height += phonetic.height + ROW_GAP;
                    }
                    PreparedLine {
                        source_index,
                        cluster_index,
                        cluster_role,
                        start: line.start,
                        end: line.end,
                        effective_end: line.end,
                        height,
                        right_aligned,
                        interlude: None,
                        kind: PreparedLineKind::Karaoke {
                            is_accompaniment: line.is_accompaniment,
                            is_rtl,
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
                    let is_rtl = contains_rtl(&line.content);
                    self.text_attrs = config.normal_attrs;
                    let text = self.prepare_plain_text(
                        &line.content,
                        config.normal_font_size,
                        config.normal_line_height,
                        content_width,
                        is_rtl,
                    );
                    let translation = if config.show_translation {
                        self.text_attrs = config.translation_attrs;
                        line.translation.as_deref().and_then(|translation| {
                            self.prepare_detail_text(
                                translation,
                                config.translation_font_size,
                                config.translation_line_height,
                                content_width,
                                is_rtl,
                            )
                        })
                    } else {
                        None
                    };
                    let mut height = text.height + config.padding_y * 2.0;
                    if let Some(translation) = &translation {
                        height += translation.height + ROW_GAP;
                    }
                    PreparedLine {
                        source_index,
                        cluster_index,
                        cluster_role,
                        start: line.start,
                        end: line.end,
                        effective_end: line.end,
                        height,
                        right_aligned: is_rtl,
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
                if line_index == 0 {
                    prepared.right_aligned
                } else {
                    previous_right_aligned
                },
                &config,
            ) {
                prepared.height += interlude.height;
                prepared.interlude = Some(interlude);
            }

            cursor_y += prepared.height;
            previous_end = Some(prepared.end);
            previous_right_aligned = prepared.right_aligned;
            lines.push(prepared);
        }

        // Split oversized scroll clusters. A main line and its nested accompaniment
        // lines normally scroll and anchor as ONE block (a shared `cluster_index`),
        // but when their combined wrapped-row count is large, pinning the block's
        // top drags the sung line well below the focus. Break such a cluster into
        // per-line scroll units: each accompaniment keeps its `cluster_role` (so the
        // entrance bloom still fires) but gets its own fresh `cluster_index`, while
        // the main line stays on the original one — so the scroll anchors each part
        // individually instead of as one tall block. Runs before the effective-end
        // pass below so a split main line only stays focused for its own span.
        const MAX_CLUSTER_ROWS: usize = 3;
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
                .is_some_and(|rows| *rows > MAX_CLUSTER_ROWS);
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

        Ok(PreparedScene {
            config,
            lines,
            content_height: cursor_y + config.keep_alive,
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
        is_rtl: bool,
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

        let wrapped = self.calculate_balanced_lines(&measured, width, font_size, line_height);
        self.position_karaoke_wrapped_lines(
            wrapped,
            syllables,
            width,
            line_height,
            phonetic_line_height,
            phonetic_gap,
            right_aligned,
            is_rtl,
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
            first_baseline: text.first_baseline,
            height: text.height,
            text: draw_text,
            phonetic: phonetic_text,
            width,
        }
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
                if j > 1 && syllable_layouts[j - 2].word_id == syllable_layouts[j - 1].word_id {
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
                if layout.word_id != current_word_id {
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
        is_rtl: bool,
    ) -> PreparedText {
        let mut rows = Vec::new();
        let mut first_baseline = None;
        let mut bounds_by_word = HashMap::<usize, (f32, f32, f32)>::new();
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
            let mut current_x = if is_rtl {
                start_x + wrapped_line.total_width
            } else {
                start_x
            };
            let mut row_glyphs = Vec::new();

            for layout in wrapped_line.syllables {
                let position_x = if is_rtl {
                    current_x - layout.width
                } else {
                    current_x
                };
                let vertical_offset = max_baseline - layout.first_baseline;
                let position_y = row_top_y + vertical_offset;
                let bottom_y = position_y + layout.height;
                if let Some(syllable) = syllables.get_mut(layout.index) {
                    syllable.layout_x = position_x;
                    syllable.layout_width = layout.width;
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

                if is_rtl {
                    current_x -= layout.width;
                } else {
                    current_x += layout.width;
                }
            }

            rows.push(PreparedRow {
                y: row_top_y,
                width: wrapped_line.total_width,
                min_x: start_x,
                max_x: start_x + wrapped_line.total_width,
                glyphs: row_glyphs,
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
                let syllable_index = (glyph.metadata > 0).then_some(glyph.metadata - 1);
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
