use super::lyrics_parser::LyricsParser;
use crate::model::{
    karaoke::strip_enclosing_parentheses, AccompanimentKaraokeLine, KaraokeAlignment,
    KaraokeSyllable, MainKaraokeLine, SyncedLineKind, SyncedLyrics,
};
use crate::utils::kugou_krc_metadata_decoder;
use regex::Regex;
use std::sync::OnceLock;

pub struct KugouKrcParser;

const LANGUAGE_TAG_START: &str = "[language:";

impl LyricsParser for KugouKrcParser {
    fn can_parse(&self, content: &str) -> bool {
        content
            .lines()
            .map(str::trim)
            .any(|line| line_time_regex().is_match(line) && word_time_regex().is_match(line))
    }

    fn parse_lines(&self, lines: &[String]) -> SyncedLyrics {
        parse_internal(lines.iter().map(|line| line.as_str()))
    }

    fn parse(&self, content: &str) -> SyncedLyrics {
        parse_internal(content.lines())
    }
}

fn parse_internal<'a>(raw_lines_sequence: impl Iterator<Item = &'a str>) -> SyncedLyrics {
    let raw_lines = raw_lines_sequence.collect::<Vec<_>>();
    let language_line = raw_lines
        .iter()
        .find(|line| line.trim().starts_with(LANGUAGE_TAG_START))
        .copied();
    let metadata = kugou_krc_metadata_decoder::decode(language_line);
    let mut result_lines = Vec::<SyncedLineKind>::new();

    let mut current_role_state = KaraokeAlignment::Start;
    let mut lyric_line_index = 0usize;
    let mut last_line_start_time = -1;

    for raw in raw_lines {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(LANGUAGE_TAG_START) {
            continue;
        }

        if line.starts_with("[bg:") {
            if let Some(bg_line) = parse_background_line(line) {
                if let Some(SyncedLineKind::MainKaraoke(last)) = result_lines.last_mut() {
                    last.accompaniment_lines
                        .get_or_insert_with(Vec::new)
                        .push(bg_line);
                } else {
                    result_lines.push(bg_line.into());
                }
            }
            continue;
        }

        let Some(captures) = krc_line_regex().captures(line) else {
            continue;
        };
        let mut line_start = captures
            .get(1)
            .and_then(|m| m.as_str().parse::<i32>().ok())
            .unwrap_or(0);
        if last_line_start_time != -1 && line_start <= last_line_start_time {
            line_start = last_line_start_time + 3;
        }
        last_line_start_time = line_start;

        let content_part = captures.get(3).map(|m| m.as_str()).unwrap_or("");
        let raw_syllables = parse_syllables_and_merge_colons(content_part, line_start);
        let syllables_with_phonetics =
            inject_phonetics(raw_syllables, &metadata.phonetics, lyric_line_index);
        let (alignment, final_syllables, next_state) =
            determine_role(syllables_with_phonetics, current_role_state);
        current_role_state = next_state;

        let translation = metadata
            .translations
            .get(lyric_line_index)
            .filter(|value| !value.trim().is_empty())
            .cloned();

        if !final_syllables.is_empty() {
            let start = final_syllables.first().unwrap().start;
            let end = final_syllables.last().unwrap().end;
            result_lines.push(
                MainKaraokeLine::new(final_syllables, translation, alignment, start, end).into(),
            );
        }
        lyric_line_index += 1;
    }

    SyncedLyrics::new(result_lines)
}

fn parse_background_line(line: &str) -> Option<AccompanimentKaraokeLine> {
    let captures = bg_line_regex().captures(line)?;
    let content = captures.get(1)?.as_str();
    let syllables = strip_enclosing_parentheses(parse_syllables_and_merge_colons(content, 0));
    if syllables.is_empty() {
        return None;
    }
    Some(AccompanimentKaraokeLine::new(
        syllables.clone(),
        None,
        KaraokeAlignment::Unspecified,
        syllables.first()?.start,
        syllables.last()?.end,
    ))
}

fn inject_phonetics(
    syllables: Vec<KaraokeSyllable>,
    all_phonetics: &[Vec<String>],
    line_index: usize,
) -> Vec<KaraokeSyllable> {
    let Some(line_phonetics) = all_phonetics.get(line_index) else {
        return syllables;
    };
    if line_phonetics.len() != syllables.len() {
        return syllables;
    }
    syllables
        .into_iter()
        .zip(line_phonetics.iter())
        .map(|(mut syllable, phonetic)| {
            syllable.phonetic = Some(phonetic.clone());
            syllable
        })
        .collect()
}

fn parse_syllables_and_merge_colons(content: &str, base_start_time: i32) -> Vec<KaraokeSyllable> {
    #[derive(Clone)]
    struct TempToken {
        offset: i32,
        duration: i32,
        text: String,
    }

    let mut tokens = Vec::<TempToken>::new();
    let mut cursor = 0usize;
    while cursor < content.len() {
        let Some(captures) = syllable_regex().captures(&content[cursor..]) else {
            break;
        };
        let whole = captures.get(0).unwrap();
        let absolute_start = cursor + whole.start();
        let absolute_end = cursor + whole.end();
        let offset = captures
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let duration = captures
            .get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let text_start = absolute_end;
        let next_match = syllable_regex().find(&content[text_start..]);
        let text_end = next_match
            .map(|m| text_start + m.start())
            .unwrap_or(content.len());
        if text_start > text_end || absolute_start >= content.len() {
            break;
        }
        tokens.push(TempToken {
            offset,
            duration,
            text: content[text_start..text_end].to_string(),
        });
        cursor = text_end;
    }

    let mut merged = Vec::new();
    let mut i = 0usize;
    while i < tokens.len() {
        let current = &tokens[i];
        let next = tokens.get(i + 1);
        if next.is_some_and(|token| token.text == "：" || token.text == ":") {
            let next = next.unwrap();
            let start = base_start_time + current.offset;
            merged.push(KaraokeSyllable::new(
                format!("{}{}", current.text, next.text),
                start,
                start + current.duration + next.duration,
            ));
            i += 2;
        } else {
            let start = base_start_time + current.offset;
            merged.push(KaraokeSyllable::new(
                current.text.clone(),
                start,
                start + current.duration,
            ));
            i += 1;
        }
    }
    merged
}

fn determine_role(
    syllables: Vec<KaraokeSyllable>,
    current_state: KaraokeAlignment,
) -> (KaraokeAlignment, Vec<KaraokeSyllable>, KaraokeAlignment) {
    if syllables.is_empty() {
        return (KaraokeAlignment::Unspecified, syllables, current_state);
    }

    let raw_text = syllables
        .iter()
        .map(|syllable| syllable.content.as_str())
        .collect::<String>();
    let has_marker = raw_text.starts_with('：')
        || raw_text.starts_with(':')
        || raw_text.ends_with('：')
        || raw_text.ends_with(':');
    if has_marker {
        let new_state = if current_state == KaraokeAlignment::Start {
            KaraokeAlignment::End
        } else {
            KaraokeAlignment::Start
        };
        return (new_state, syllables, new_state);
    }
    (current_state, syllables, current_state)
}

fn line_time_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^\[\d+,\d+\]").unwrap())
}

fn word_time_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"<\d+,\d+,\d+>.{1}").unwrap())
}

fn krc_line_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^\[(\d+),(\d+)\](.*)$").unwrap())
}

fn syllable_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"<(\d+),(\d+),\d+>").unwrap())
}

fn bg_line_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^\[bg:(.*)\](.*)$").unwrap())
}
