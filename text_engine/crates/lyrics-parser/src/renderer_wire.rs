use crate::model::{KaraokeAlignment, SyncedLineKind, SyncedLyrics};

/// Host-supplied layout knobs for building a render scene that matches the Android
/// player (density-scaled style + top bar, safe-area insets for chrome).
#[derive(Clone, Debug)]
pub struct SceneBuildParams {
    pub width: u32,
    pub height: u32,
    /// Physical pixels per density-independent pixel (window scale factor / Android density).
    pub density: f32,
    /// System top inset in physical px (status bar / desktop caption bar).
    pub content_top_inset: f32,
    pub content_bottom_inset: f32,
    pub content_left_inset: f32,
    pub content_right_inset: f32,
    /// Prefer these for the in-surface player top bar (e.g. SMTC / media session).
    /// Falls back to lyrics metadata when absent.
    pub top_bar_title: Option<String>,
    pub top_bar_artist: Option<String>,
}

impl SceneBuildParams {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            density: 1.0,
            content_top_inset: 0.0,
            content_bottom_inset: 0.0,
            content_left_inset: 0.0,
            content_right_inset: 0.0,
            top_bar_title: None,
            top_bar_artist: None,
        }
    }

    pub fn with_density(mut self, density: f32) -> Self {
        self.density = density.max(0.5);
        self
    }

    pub fn with_insets(mut self, top: f32, bottom: f32, left: f32, right: f32) -> Self {
        self.content_top_inset = top.max(0.0);
        self.content_bottom_inset = bottom.max(0.0);
        self.content_left_inset = left.max(0.0);
        self.content_right_inset = right.max(0.0);
        self
    }

    pub fn with_top_bar(mut self, title: impl Into<String>, artist: impl Into<String>) -> Self {
        let title = title.into();
        if title.trim().is_empty() {
            self.top_bar_title = None;
            self.top_bar_artist = None;
        } else {
            self.top_bar_title = Some(title);
            self.top_bar_artist = Some(artist.into());
        }
        self
    }
}

pub fn scene_json(lyrics: &SyncedLyrics, width: u32, height: u32) -> String {
    scene_json_with(lyrics, &SceneBuildParams::new(width, height))
}

pub fn scene_json_with(lyrics: &SyncedLyrics, params: &SceneBuildParams) -> String {
    let width = params.width.max(1);
    let height = params.height.max(1);
    let density = params.density.max(0.5);
    let (top_bar, content_top) = top_bar_json(lyrics, width, density, params);
    let mut lines = String::new();
    let mut source_index = 0usize;
    for line in flatten_lines(&lyrics.lines) {
        if source_index > 0 {
            lines.push(',');
        }
        match line {
            FlatLine::Synced {
                content,
                translation,
                start,
                end,
            } => {
                lines.push_str(&format!(
                    "{{\"kind\":\"synced\",\"sourceIndex\":{source_index},\"clusterIndex\":{source_index},\"clusterRole\":\"standalone\",\"start\":{},\"end\":{},\"content\":{},\"translation\":{}}}",
                    start,
                    end,
                    json_string(content),
                    json_option(translation.as_deref())
                ));
            }
            FlatLine::Karaoke {
                syllables,
                translation,
                alignment,
                start,
                end,
                phonetic,
                is_accompaniment,
                cluster_role,
                cluster_index,
            } => {
                let syllables_json = syllables
                    .iter()
                    .map(|syllable| {
                        format!(
                            "{{\"content\":{},\"start\":{},\"end\":{},\"phonetic\":{}}}",
                            json_string(&syllable.content),
                            syllable.start,
                            syllable.end,
                            json_option(syllable.phonetic.as_deref())
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                lines.push_str(&format!(
                    "{{\"kind\":\"karaoke\",\"sourceIndex\":{source_index},\"clusterIndex\":{cluster_index},\"clusterRole\":\"{cluster_role}\",\"start\":{},\"end\":{},\"isAccompaniment\":{},\"alignment\":\"{}\",\"translation\":{},\"phonetic\":{},\"syllables\":[{}]}}",
                    start,
                    end,
                    is_accompaniment,
                    alignment_wire(*alignment),
                    json_option(translation.as_deref()),
                    json_option(phonetic.as_deref()),
                    syllables_json
                ));
            }
        }
        source_index += 1;
    }

    let locale_text = lyrics
        .lines
        .iter()
        .map(SyncedLineKind::content_string)
        .collect::<String>();

    format!(
        "{{\"width\":{},\"height\":{},\"locale\":{},\"contentTop\":{},\"contentBottom\":{},\"contentLeft\":{},\"contentRight\":{},\"topBar\":{},\"style\":{},\"lines\":[{}]}}",
        width,
        height,
        json_string(detect_locale(&locale_text)),
        number(content_top),
        number(params.content_bottom_inset),
        number(params.content_left_inset),
        number(params.content_right_inset),
        top_bar,
        android_style_json(density),
        lines
    )
}

enum FlatLine<'a> {
    Synced {
        content: &'a str,
        translation: &'a Option<String>,
        start: i32,
        end: i32,
    },
    Karaoke {
        syllables: &'a [crate::model::KaraokeSyllable],
        translation: &'a Option<String>,
        alignment: &'a KaraokeAlignment,
        start: i32,
        end: i32,
        phonetic: &'a Option<String>,
        is_accompaniment: bool,
        cluster_role: &'static str,
        cluster_index: usize,
    },
}

fn flatten_lines(lines: &[SyncedLineKind]) -> Vec<FlatLine<'_>> {
    let mut result = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        match line {
            SyncedLineKind::Synced(line) => result.push(FlatLine::Synced {
                content: &line.content,
                translation: &line.translation,
                start: line.start,
                end: line.end,
            }),
            SyncedLineKind::MainKaraoke(line) => {
                let main_vocal_start = line
                    .syllables
                    .first()
                    .map(|syllable| syllable.start)
                    .unwrap_or(line.start);
                if let Some(accompaniment) = &line.accompaniment_lines {
                    for bg in accompaniment
                        .iter()
                        .filter(|bg| bg.start < main_vocal_start)
                    {
                        result.push(FlatLine::Karaoke {
                            syllables: &bg.syllables,
                            translation: &bg.translation,
                            alignment: &bg.alignment,
                            start: bg.start,
                            end: bg.end,
                            phonetic: &bg.phonetic,
                            is_accompaniment: true,
                            cluster_role: "before_accompaniment",
                            cluster_index: index,
                        });
                    }
                }
                result.push(FlatLine::Karaoke {
                    syllables: &line.syllables,
                    translation: &line.translation,
                    alignment: &line.alignment,
                    start: line.start,
                    end: line.end,
                    phonetic: &line.phonetic,
                    is_accompaniment: false,
                    cluster_role: "main",
                    cluster_index: index,
                });
                if let Some(accompaniment) = &line.accompaniment_lines {
                    for bg in accompaniment
                        .iter()
                        .filter(|bg| bg.start >= main_vocal_start)
                    {
                        result.push(FlatLine::Karaoke {
                            syllables: &bg.syllables,
                            translation: &bg.translation,
                            alignment: &bg.alignment,
                            start: bg.start,
                            end: bg.end,
                            phonetic: &bg.phonetic,
                            is_accompaniment: true,
                            cluster_role: "after_accompaniment",
                            cluster_index: index,
                        });
                    }
                }
            }
            SyncedLineKind::AccompanimentKaraoke(line) => result.push(FlatLine::Karaoke {
                syllables: &line.syllables,
                translation: &line.translation,
                alignment: &line.alignment,
                start: line.start,
                end: line.end,
                phonetic: &line.phonetic,
                is_accompaniment: true,
                cluster_role: "standalone",
                cluster_index: index,
            }),
        }
    }
    result
}

/// Android `KaraokeLyricsConfig` defaults, density-scaled the same way as
/// `KaraokeLyricsConfig.toSceneStyle(Density)` + optional render-target scale.
fn android_style_json(density: f32) -> String {
    let d = density.max(0.5);
    format!(
        "{{\
\"typography\":{{\
\"normalFontSize\":{},\"normalLineHeight\":{},\"normalFontWeight\":700,\"normalFontItalic\":false,\
\"accompanimentFontSize\":{},\"accompanimentLineHeight\":{},\"accompanimentFontWeight\":700,\"accompanimentFontItalic\":false,\
\"translationFontSize\":{},\"translationLineHeight\":{},\"translationFontWeight\":400,\"translationFontItalic\":false,\
\"accompanimentTranslationFontSize\":{},\"accompanimentTranslationLineHeight\":{},\"accompanimentTranslationFontWeight\":400,\"accompanimentTranslationFontItalic\":false,\
\"phoneticFontSize\":{},\"phoneticLineHeight\":{},\"phoneticFontWeight\":400,\"phoneticFontItalic\":false\
}},\
\"spacing\":{{\
\"horizontalPadding\":{},\"linePadding\":{},\"accompanimentGap\":{},\"phoneticGap\":{},\
\"focusTopOffset\":{},\"translationGap\":{},\"accompanimentTranslationGap\":{}\
}},\
\"blur\":{{\"enabled\":true,\"delta\":{},\"sharpRadiusLines\":2.5}},\
\"focus\":{{\"inactiveKaraokeAlpha\":0.2,\"dimMinAlpha\":0.2,\"dimFalloffMs\":400}},\
\"autoScrollSpring\":{{\"stiffness\":80,\"damping\":14,\"chainCoupling\":0.65,\"distanceFalloff\":0.2,\"minResponse\":0.35}},\
\"manualScroll\":{{\"maxFlingVelocity\":14000,\"decelerationRate\":0.998,\"overscrollStiffness\":119.4,\"overscrollDamping\":21.85,\"rubberBandLimit\":180,\"rubberBandCoefficient\":0.55,\"blurRestoreMs\":2500,\"blurFadeInRate\":6,\"blurFadeOutRate\":12}},\
\"breathingDots\":{{\"number\":3,\"size\":{},\"margin\":{},\"enterMs\":3000,\"stillMs\":200,\"dipMs\":3000,\"exitMs\":200,\"color\":4294967295}},\
\"textColor\":4294967295,\"showTranslation\":true,\"showPhonetic\":true\
}}",
        number(34.0 * d),
        number(40.0 * d),
        number(20.0 * d),
        number(26.0 * d),
        number(16.0 * d),
        number(20.0 * d),
        number(12.0 * d),
        number(16.0 * d),
        number(24.0 * d),
        number(30.0 * d),
        number(28.0 * d),
        number(12.0 * d),
        number(8.0 * d),
        number(8.0 * d),
        number(50.0 * d),
        number(8.0 * d),
        number(4.0 * d),
        number(1.0 * d),
        number(16.0 * d),
        number(12.0 * d),
    )
}

/// Mirror of Android `RustSkiaLyricsView.resolveTopBar`: 28dp padding, 68dp thumb,
/// 17sp/15sp text, ⋯ button on the right. `density` is Android density / desktop scale.
fn top_bar_json(
    lyrics: &SyncedLyrics,
    width: u32,
    density: f32,
    params: &SceneBuildParams,
) -> (String, f32) {
    let title = params
        .top_bar_title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .or_else(|| {
            let title = lyrics.title.trim();
            if title.is_empty() {
                None
            } else {
                Some(title.to_string())
            }
        });

    let Some(title) = title else {
        return ("null".to_string(), params.content_top_inset);
    };

    let artist = params
        .top_bar_artist
        .clone()
        .unwrap_or_else(|| {
            lyrics
                .artists
                .iter()
                .map(|artist| artist.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        });

    let d = density.max(0.5);
    let dp = |v: f32| v * d;
    // Desktop has no separate fontScale; match Android at fontScale=1.
    let sp = |v: f32| v * d;

    let sys_top = params.content_top_inset;
    let sys_left = params.content_left_inset;
    let sys_right = params.content_right_inset;

    let bar_top = sys_top + dp(28.0);
    let thumb_size = dp(68.0);
    let thumb_left = sys_left + dp(28.0);
    let gap = dp(8.0);
    let text_left = thumb_left + thumb_size + gap;
    let title_font_size = sp(17.0);
    let artist_font_size = sp(15.0);
    let title_line_height = title_font_size * 1.3;
    let artist_line_height = artist_font_size * 1.3;
    let text_block_height = title_line_height + artist_line_height;
    let title_top = bar_top + (thumb_size - text_block_height) / 2.0;
    let artist_top = title_top + title_line_height;
    let button_radius = dp(14.0);
    let button_cx = width as f32 - sys_right - dp(28.0) - button_radius;
    let button_cy = bar_top + thumb_size / 2.0;
    let text_max_width = (button_cx - button_radius - gap - text_left).max(1.0);
    let content_top = bar_top + thumb_size + dp(20.0);

    let json = format!(
        "{{\"title\":{},\"artist\":{},\"thumbLeft\":{},\"thumbTop\":{},\"thumbSize\":{},\"thumbRadius\":{},\"textLeft\":{},\"textMaxWidth\":{},\"titleTop\":{},\"titleFontSize\":{},\"titleLineHeight\":{},\"titleWeight\":600,\"artistTop\":{},\"artistFontSize\":{},\"artistLineHeight\":{},\"artistAlpha\":0.4,\"buttonCx\":{},\"buttonCy\":{},\"buttonRadius\":{}}}",
        json_string(&title),
        json_string(&artist),
        number(thumb_left),
        number(bar_top),
        number(thumb_size),
        number(dp(14.0)),
        number(text_left),
        number(text_max_width),
        number(title_top),
        number(title_font_size),
        number(title_line_height),
        number(artist_top),
        number(artist_font_size),
        number(artist_line_height),
        number(button_cx),
        number(button_cy),
        number(button_radius)
    );
    (json, content_top)
}

fn detect_locale(text: &str) -> &'static str {
    if text
        .chars()
        .any(|ch| ('\u{3040}'..='\u{30ff}').contains(&ch))
    {
        "ja-JP"
    } else if text
        .chars()
        .any(|ch| ('\u{ac00}'..='\u{d7af}').contains(&ch))
    {
        "ko-KR"
    } else if text
        .chars()
        .any(|ch| ('\u{3400}'..='\u{9fff}').contains(&ch))
    {
        "zh-CN"
    } else {
        "en-US"
    }
}

fn alignment_wire(alignment: KaraokeAlignment) -> &'static str {
    match alignment {
        KaraokeAlignment::Start => "start",
        KaraokeAlignment::End => "end",
        KaraokeAlignment::Unspecified => "unspecified",
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn json_option(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_string())
}

fn number(value: f32) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    if value.fract().abs() < f32::EPSILON {
        format!("{}", value as i32)
    } else {
        format!("{value:.3}")
    }
}
