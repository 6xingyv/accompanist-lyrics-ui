use super::lyrics_parser::LyricsParser;
use crate::model::{
    karaoke::strip_enclosing_parentheses, AccompanimentKaraokeLine, KaraokeAlignment,
    KaraokeSyllable, MainKaraokeLine, SyncedLineKind, SyncedLyrics,
};
use crate::utils::lrc_metadata_helper;
use crate::utils::time_utils::is_digits_only;
use regex::Regex;
use std::sync::OnceLock;

pub struct LyricifySyllableParser;

impl LyricsParser for LyricifySyllableParser {
    fn can_parse(&self, content: &str) -> bool {
        detector_regex().is_match(content)
    }

    fn parse_lines(&self, lines: &[String]) -> SyncedLyrics {
        let lyrics_lines = lrc_metadata_helper::remove_attributes(lines);
        let mut data = Vec::<SyncedLineKind>::new();

        for line in lyrics_lines.iter().filter(|line| !line.trim().is_empty()) {
            let parsed = parse_line(line);
            if let SyncedLineKind::AccompanimentKaraoke(bg) = parsed.clone() {
                if let Some(SyncedLineKind::MainKaraoke(last)) = data.last_mut() {
                    last.accompaniment_lines
                        .get_or_insert_with(Vec::new)
                        .push(bg);
                } else {
                    data.push(parsed);
                }
            } else {
                data.push(parsed);
            }
        }

        SyncedLyrics::new(data)
    }
}

fn parse_line(line: &str) -> SyncedLineKind {
    let (real, is_accompaniment, alignment) = if line.contains(']')
        && line.contains('[')
        && line.find(']').unwrap() - line.find('[').unwrap() == 2
    {
        let real = &line[line.find(']').unwrap() + 1..];
        let attribute = attribute_regex()
            .captures(line)
            .and_then(|captures| captures.get(1))
            .and_then(|m| m.as_str().parse::<i32>().ok())
            .unwrap_or(0);
        let is_accompaniment = !(0..=5).contains(&attribute);
        let alignment = if attribute == 2 || attribute == 5 || attribute == 8 {
            KaraokeAlignment::End
        } else {
            KaraokeAlignment::Start
        };
        (real, is_accompaniment, alignment)
    } else {
        (line, false, KaraokeAlignment::Start)
    };

    let syllables = syllable_regex()
        .captures_iter(real)
        .filter_map(|captures| {
            let content = captures.get(1)?.as_str().to_string();
            let start = captures.get(2)?.as_str();
            let duration = captures.get(3)?.as_str();
            if is_digits_only(start) && is_digits_only(duration) {
                let start = start.parse::<i32>().ok()?;
                let duration = duration.parse::<i32>().ok()?;
                Some(KaraokeSyllable::new(content, start, start + duration))
            } else {
                Some(KaraokeSyllable::new("Error".to_string(), 0, 0))
            }
        })
        .collect::<Vec<_>>();

    let start_time = syllables.first().map(|s| s.start).unwrap_or(0);
    let end_time = syllables.last().map(|s| s.end).unwrap_or(0);

    if is_accompaniment {
        AccompanimentKaraokeLine::new(
            strip_enclosing_parentheses(syllables),
            None,
            alignment,
            start_time,
            end_time,
        )
        .into()
    } else {
        MainKaraokeLine::new(syllables, None, alignment, start_time, end_time).into()
    }
}

fn detector_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"[a-zA-Z]+\s*\(\d+,\d+\)").unwrap())
}

fn syllable_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(.*?)\((\d+),(\d+)\)").unwrap())
}

fn attribute_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\[(\d+)\]").unwrap())
}
