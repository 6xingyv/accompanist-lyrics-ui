use super::lyrics_parser::LyricsParser;
use crate::model::{
    KaraokeAlignment, KaraokeSyllable, MainKaraokeLine, SyncedLine, SyncedLineKind, SyncedLyrics,
};
use regex::Regex;
use std::sync::OnceLock;

pub struct NeteaseYrcParser;

impl LyricsParser for NeteaseYrcParser {
    fn can_parse(&self, content: &str) -> bool {
        content.lines().any(|line| {
            let trimmed = line.trim();
            let Some(captures) = line_regex().captures(trimmed) else {
                return false;
            };
            syllable_regex().is_match(captures.get(3).map(|m| m.as_str()).unwrap_or(""))
        })
    }

    fn parse_lines(&self, lines: &[String]) -> SyncedLyrics {
        SyncedLyrics::new(lines.iter().filter_map(|line| parse_line(line)).collect())
    }
}

fn parse_line(raw_line: &str) -> Option<SyncedLineKind> {
    let line = raw_line.trim();
    if line.is_empty() || line.starts_with('{') {
        return None;
    }

    let captures = line_regex().captures(line)?;
    let line_start = captures.get(1)?.as_str().parse::<i32>().ok()?;
    let line_duration = captures.get(2)?.as_str().parse::<i32>().ok()?;
    let line_end = line_start + line_duration;
    let content = captures.get(3)?.as_str();

    let raw_syllables = syllable_regex()
        .captures_iter(content)
        .filter_map(|captures| {
            let raw_start = captures.get(1)?.as_str().parse::<i32>().ok()?;
            let duration = captures.get(2)?.as_str().parse::<i32>().ok()?;
            Some(KaraokeSyllable::new(
                captures.get(3)?.as_str().to_string(),
                raw_start,
                raw_start + duration,
            ))
        })
        .collect::<Vec<_>>();

    if raw_syllables.is_empty() {
        let plain_text = content.trim();
        return (!plain_text.is_empty())
            .then(|| SyncedLine::new(plain_text.to_string(), None, line_start, line_end).into());
    }

    let syllables = normalize_syllable_times(raw_syllables, line_start);
    let effective_start = syllables.first()?.start;
    let effective_end = line_end.max(syllables.last()?.end);
    Some(
        MainKaraokeLine::new(
            syllables,
            None,
            KaraokeAlignment::Unspecified,
            effective_start,
            effective_end,
        )
        .into(),
    )
}

fn normalize_syllable_times(
    syllables: Vec<KaraokeSyllable>,
    line_start: i32,
) -> Vec<KaraokeSyllable> {
    let uses_relative_time = syllables.first().is_some_and(|s| s.start < line_start);
    if !uses_relative_time {
        return syllables;
    }
    syllables
        .into_iter()
        .map(|mut syllable| {
            syllable.start += line_start;
            syllable.end += line_start;
            syllable
        })
        .collect()
}

fn line_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^\[(\d+),\s*(\d+)\](.*)$").unwrap())
}

fn syllable_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\((\d+),\s*(\d+),\s*-?\d+\)([^()\r\n]*)").unwrap())
}
