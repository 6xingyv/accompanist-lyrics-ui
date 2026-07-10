use crate::model::{KaraokeAlignment, SyncedLineKind, SyncedLyrics};

pub fn scene_json(lyrics: &SyncedLyrics, width: u32, height: u32) -> String {
    let width = width.max(1);
    let height = height.max(1);
    let (top_bar, content_top) = top_bar_json(lyrics, width);
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
        "{{\"width\":{},\"height\":{},\"locale\":{},\"contentTop\":{},\"contentBottom\":0,\"contentLeft\":0,\"contentRight\":0,\"topBar\":{},\"style\":{},\"lines\":[{}]}}",
        width,
        height,
        json_string(detect_locale(&locale_text)),
        number(content_top),
        top_bar,
        android_style_json(),
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

fn android_style_json() -> &'static str {
    "{\"typography\":{\"normalFontSize\":34,\"normalLineHeight\":40,\"normalFontWeight\":700,\"normalFontItalic\":false,\"accompanimentFontSize\":20,\"accompanimentLineHeight\":26,\"accompanimentFontWeight\":700,\"accompanimentFontItalic\":false,\"translationFontSize\":16,\"translationLineHeight\":20,\"translationFontWeight\":400,\"translationFontItalic\":false,\"accompanimentTranslationFontSize\":12,\"accompanimentTranslationLineHeight\":16,\"accompanimentTranslationFontWeight\":400,\"accompanimentTranslationFontItalic\":false,\"phoneticFontSize\":24,\"phoneticLineHeight\":30,\"phoneticFontWeight\":400,\"phoneticFontItalic\":false},\"spacing\":{\"horizontalPadding\":28,\"linePadding\":12,\"accompanimentGap\":8,\"phoneticGap\":8,\"focusTopOffset\":50,\"translationGap\":8,\"accompanimentTranslationGap\":4},\"blur\":{\"enabled\":true,\"delta\":1,\"sharpRadiusLines\":2.5},\"focus\":{\"inactiveKaraokeAlpha\":0.2,\"dimMinAlpha\":0.2,\"dimFalloffMs\":400},\"autoScrollSpring\":{\"stiffness\":80,\"damping\":14,\"chainCoupling\":0.65,\"distanceFalloff\":0.2,\"minResponse\":0.35},\"manualScroll\":{\"maxFlingVelocity\":14000,\"decelerationRate\":0.998,\"overscrollStiffness\":119.4,\"overscrollDamping\":21.85,\"rubberBandLimit\":180,\"rubberBandCoefficient\":0.55,\"blurRestoreMs\":2500,\"blurFadeInRate\":6,\"blurFadeOutRate\":12},\"breathingDots\":{\"number\":3,\"size\":16,\"margin\":12,\"enterMs\":3000,\"stillMs\":200,\"dipMs\":3000,\"exitMs\":200,\"color\":4294967295},\"textColor\":4294967295,\"showTranslation\":true,\"showPhonetic\":true}"
}

fn top_bar_json(lyrics: &SyncedLyrics, width: u32) -> (String, f32) {
    if lyrics.title.trim().is_empty() {
        return ("null".to_string(), 0.0);
    }
    let artist = lyrics
        .artists
        .iter()
        .map(|artist| artist.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let bar_top = 28.0;
    let thumb_size = 68.0;
    let thumb_left = 28.0;
    let gap = 8.0;
    let text_left = thumb_left + thumb_size + gap;
    let title_font_size = 17.0;
    let artist_font_size = 15.0;
    let title_line_height = title_font_size * 1.3;
    let artist_line_height = artist_font_size * 1.3;
    let text_block_height = title_line_height + artist_line_height;
    let title_top = bar_top + (thumb_size - text_block_height) / 2.0;
    let artist_top = title_top + title_line_height;
    let button_radius = 14.0;
    let button_cx = width as f32 - 28.0 - button_radius;
    let button_cy = bar_top + thumb_size / 2.0;
    let text_max_width = (button_cx - button_radius - gap - text_left).max(1.0);
    let content_top = bar_top + thumb_size + 20.0;

    let json = format!(
        "{{\"title\":{},\"artist\":{},\"thumbLeft\":{},\"thumbTop\":{},\"thumbSize\":{},\"thumbRadius\":14,\"textLeft\":{},\"textMaxWidth\":{},\"titleTop\":{},\"titleFontSize\":{},\"titleLineHeight\":{},\"titleWeight\":600,\"artistTop\":{},\"artistFontSize\":{},\"artistLineHeight\":{},\"artistAlpha\":0.4,\"buttonCx\":{},\"buttonCy\":{},\"buttonRadius\":{}}}",
        json_string(&lyrics.title),
        json_string(&artist),
        number(thumb_left),
        number(bar_top),
        number(thumb_size),
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
    if value.fract().abs() < f32::EPSILON {
        format!("{}", value as i32)
    } else {
        format!("{value:.3}")
    }
}
