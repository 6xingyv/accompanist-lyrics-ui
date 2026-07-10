use super::lyrics_parser::LyricsParser;
use crate::model::{
    karaoke::strip_enclosing_parentheses, AccompanimentKaraokeLine, Artist, KaraokeAlignment,
    KaraokeSyllable, MainKaraokeLine, SyncedLine, SyncedLineKind, SyncedLyrics,
};
use crate::utils::{lrc_metadata_helper, parse_as_time};
use regex::Regex;
use std::sync::OnceLock;

pub struct EnhancedLrcParser;

#[derive(Clone, Copy)]
enum BracketType {
    Angle,
    Square,
}

impl LyricsParser for EnhancedLrcParser {
    fn can_parse(&self, content: &str) -> bool {
        line_timestamp_regex().is_match(content)
    }

    fn parse_lines(&self, lines: &[String]) -> SyncedLyrics {
        let lyrics_lines = lrc_metadata_helper::remove_attributes(lines)
            .into_iter()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>();

        let raw_data = rearrange_unchecked_line_time(rearrange_accompaniment_alignment(
            combine_raw_with_translation(
                lyrics_lines
                    .iter()
                    .flat_map(|line| parse_line(line))
                    .collect::<Vec<_>>(),
            ),
        ));

        let mut data = Vec::<SyncedLineKind>::new();
        for line in raw_data {
            if let SyncedLineKind::AccompanimentKaraoke(bg) = line.clone() {
                if let Some(SyncedLineKind::MainKaraoke(last)) = data.last_mut() {
                    last.accompaniment_lines
                        .get_or_insert_with(Vec::new)
                        .push(bg);
                } else {
                    data.push(line);
                }
            } else {
                data.push(line);
            }
        }

        let attributes = lrc_metadata_helper::parse(lines);
        let artists = attributes
            .artist
            .as_deref()
            .map(|artist| {
                artist
                    .split('/')
                    .map(|part| {
                        if let Some((kind, name)) = part.split_once(':') {
                            Artist {
                                kind: kind.to_string(),
                                name: name.to_string(),
                            }
                        } else {
                            Artist {
                                kind: "Main".to_string(),
                                name: part.to_string(),
                            }
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        SyncedLyrics {
            lines: data,
            title: attributes.title.unwrap_or_default(),
            id: "0".to_string(),
            artists,
        }
    }
}

fn parse_line(string: &str) -> Vec<SyncedLineKind> {
    if string.trim().is_empty() {
        return Vec::new();
    }

    let matches = tag_regex().find_iter(string).collect::<Vec<_>>();
    if matches.is_empty() {
        return Vec::new();
    }

    let mut last_end = 0usize;
    let mut leading_tags = Vec::new();
    for m in matches {
        let prefix = &string[last_end..m.start()];
        if prefix.trim().is_empty() {
            leading_tags.push((m.start(), m.end()));
            last_end = m.end();
        } else {
            break;
        }
    }
    if leading_tags.is_empty() {
        return Vec::new();
    }

    let content = if last_end < string.len() {
        string[last_end..].trim()
    } else {
        ""
    };

    let mut results = Vec::new();
    let mut timestamps = Vec::new();
    let mut bg_tag = None::<String>;

    for (start, end) in leading_tags {
        let tag_content_raw = string[start + 1..end - 1].trim();
        if let Some(bg) = tag_content_raw.strip_prefix("bg:") {
            bg_tag = Some(bg.trim().to_string());
        } else if is_timestamp(tag_content_raw) {
            timestamps.push(parse_as_time(tag_content_raw));
        }
    }

    let bracket_type = detect_bracket_type(content, bg_tag.as_deref());
    let bg_syllables = bg_tag
        .as_deref()
        .map(|tag| strip_enclosing_parentheses(procedural_parse_syllables(tag, bracket_type)))
        .unwrap_or_default();
    let main_syllables = if !timestamps.is_empty() && !content.is_empty() {
        procedural_parse_syllables(content, bracket_type)
    } else {
        Vec::new()
    };

    let voice_match = voice_parser().captures(content);
    let alignment = match voice_match
        .as_ref()
        .and_then(|captures| captures.get(1))
        .map(|m| m.as_str())
    {
        Some("v1") => KaraokeAlignment::Start,
        Some("v2") => KaraokeAlignment::End,
        _ => KaraokeAlignment::Unspecified,
    };
    let text_content = voice_match
        .as_ref()
        .and_then(|captures| captures.get(2))
        .map(|m| m.as_str().trim())
        .unwrap_or(content);

    let first_timestamp = timestamps.first().copied().unwrap_or(0);
    let is_relative = main_syllables
        .first()
        .is_some_and(|syllable| syllable.start < first_timestamp);
    let bg_is_relative = bg_syllables
        .first()
        .is_some_and(|syllable| syllable.start < first_timestamp);

    if !timestamps.is_empty() {
        for start_time in timestamps {
            if !main_syllables.is_empty() {
                let offset = if is_relative {
                    start_time
                } else {
                    start_time - first_timestamp
                };
                let shifted = shift_syllables(&main_syllables, offset);
                results.push(
                    MainKaraokeLine::new(
                        shifted.clone(),
                        None,
                        alignment,
                        shifted.first().unwrap().start,
                        shifted.last().unwrap().end,
                    )
                    .into(),
                );
            } else if !text_content.trim().is_empty() {
                results.push(
                    SyncedLine::new(text_content.to_string(), None, start_time, start_time).into(),
                );
            }

            if !bg_syllables.is_empty() {
                let bg_offset = if bg_is_relative {
                    start_time
                } else {
                    start_time - first_timestamp
                };
                let shifted = shift_syllables(&bg_syllables, bg_offset);
                results.push(
                    AccompanimentKaraokeLine::new(
                        shifted.clone(),
                        None,
                        KaraokeAlignment::Unspecified,
                        shifted.first().unwrap().start,
                        shifted.last().unwrap().end,
                    )
                    .into(),
                );
            }
        }
    } else if !bg_syllables.is_empty() {
        results.push(
            AccompanimentKaraokeLine::new(
                bg_syllables.clone(),
                None,
                KaraokeAlignment::Unspecified,
                bg_syllables.first().unwrap().start,
                bg_syllables.last().unwrap().end,
            )
            .into(),
        );
    }

    results
}

fn procedural_parse_syllables(content: &str, bracket_type: BracketType) -> Vec<KaraokeSyllable> {
    if content.trim().is_empty() {
        return Vec::new();
    }
    let regex = match bracket_type {
        BracketType::Angle => angle_syllable_regex(),
        BracketType::Square => square_syllable_regex(),
    };
    let syllables = regex
        .captures_iter(content)
        .filter_map(|captures| {
            let ts_part = captures.get(1)?.as_str().trim();
            let text = captures.get(2)?.as_str().to_string();
            is_timestamp(ts_part)
                .then(|| KaraokeSyllable::new(text, parse_as_time(ts_part), parse_as_time(ts_part)))
        })
        .collect::<Vec<_>>();
    rearrange_syllable_time(syllables)
}

fn rearrange_syllable_time(syllables: Vec<KaraokeSyllable>) -> Vec<KaraokeSyllable> {
    if syllables.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    for index in 0..syllables.len().saturating_sub(1) {
        let mut syllable = syllables[index].clone();
        syllable.end = syllables[index + 1].start;
        result.push(syllable);
    }
    let last = syllables.last().unwrap().clone();
    if !last.content.is_empty() {
        result.push(last);
    }
    result
}

fn shift_syllables(syllables: &[KaraokeSyllable], offset: i32) -> Vec<KaraokeSyllable> {
    syllables
        .iter()
        .cloned()
        .map(|mut syllable| {
            syllable.start += offset;
            syllable.end += offset;
            syllable
        })
        .collect()
}

fn combine_raw_with_translation(lines: Vec<SyncedLineKind>) -> Vec<SyncedLineKind> {
    let mut result = Vec::new();
    let mut used = vec![false; lines.len()];
    for i in 0..lines.len() {
        if used[i] {
            continue;
        }
        let line = &lines[i];
        let content = line.content_string().trim().to_string();
        let mut translation_found = false;
        for j in i + 1..lines.len() {
            if used[j] {
                continue;
            }
            let next = &lines[j];
            if line.same_variant_or_translation_compatible(next)
                && (line.start() - next.start()).abs() <= 150
            {
                let next_content = next.content_string().trim().to_string();
                if content != next_content && !content.is_empty() {
                    result.push(line.clone().with_translation(next_content));
                    used[i] = true;
                    used[j] = true;
                    translation_found = true;
                    break;
                }
            }
        }
        if !translation_found {
            result.push(line.clone());
            used[i] = true;
        }
    }
    result
}

fn rearrange_accompaniment_alignment(lines: Vec<SyncedLineKind>) -> Vec<SyncedLineKind> {
    let mut last_alignment = KaraokeAlignment::Unspecified;
    lines
        .into_iter()
        .map(|mut line| {
            match &mut line {
                SyncedLineKind::AccompanimentKaraoke(bg) => {
                    if bg.alignment != last_alignment {
                        bg.alignment = last_alignment;
                    }
                }
                SyncedLineKind::MainKaraoke(main) => {
                    last_alignment = main.alignment;
                }
                SyncedLineKind::Synced(_) => {
                    last_alignment = KaraokeAlignment::Unspecified;
                }
            }
            line
        })
        .collect()
}

fn rearrange_unchecked_line_time(mut lines: Vec<SyncedLineKind>) -> Vec<SyncedLineKind> {
    for index in 0..lines.len() {
        let next_start = lines
            .get(index + 1)
            .map(|line| line.start())
            .unwrap_or(i32::MAX);
        if let SyncedLineKind::Synced(line) = &mut lines[index] {
            line.end = next_start;
        }
    }
    lines
}

fn detect_bracket_type(content: &str, bg_tag: Option<&str>) -> BracketType {
    if content.contains("<0") || content.contains("<1") || content.contains("<2") {
        return BracketType::Angle;
    }
    if content.contains("[0") || content.contains("[1") || content.contains("[2") {
        return BracketType::Square;
    }
    if let Some(bg) = bg_tag {
        if bg.contains("<0") || bg.contains("<1") || bg.contains("<2") {
            return BracketType::Angle;
        }
        if bg.contains("[0") || bg.contains("[1") || bg.contains("[2") {
            return BracketType::Square;
        }
    }
    BracketType::Angle
}

fn is_timestamp(value: &str) -> bool {
    timestamp_pattern().is_match(value.trim())
}

fn line_timestamp_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\[\d{2}:\d{2}\.\d{2,3}\]").unwrap())
}

fn voice_parser() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^(v\d+)\s*:\s*(.*)").unwrap())
}

fn tag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\[(.*?)\]").unwrap())
}

fn timestamp_pattern() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^\d+([:.]\d+)+$").unwrap())
}

fn angle_syllable_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"<([^>]+)>([^<]*)").unwrap())
}

fn square_syllable_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\[([^\]]+)\]([^\[]*)").unwrap())
}
