pub mod exporter;
pub mod model;
pub mod parser;
pub mod renderer_wire;
pub mod utils;

pub use model::*;
pub use parser::{
    auto_parser::AutoParser, enhanced_lrc_parser::EnhancedLrcParser, lyrics_parser::LyricsParser,
};

pub fn parse_auto(content: &str) -> model::SyncedLyrics {
    AutoParser::default().parse(content)
}

/// Parse any supported lyrics format and return searchable, presentation-free
/// text. Every main line, translation, phonetic line and accompaniment is kept
/// on its own line; timestamps and source markup are intentionally omitted.
pub fn parse_plain_text(content: &str) -> String {
    plain_text_from_lyrics(&parse_auto(content))
}

fn plain_text_from_lyrics(lyrics: &model::SyncedLyrics) -> String {
    use model::SyncedLineKind;

    let mut lines = Vec::new();
    let mut push = |value: &str| {
        let value = value.trim();
        if !value.is_empty() && lines.last().is_none_or(|last| last != value) {
            lines.push(value.to_string());
        }
    };

    for line in &lyrics.lines {
        match line {
            SyncedLineKind::Synced(line) => {
                push(&line.content);
                if let Some(translation) = &line.translation {
                    push(translation);
                }
            }
            SyncedLineKind::MainKaraoke(line) => {
                push(&model::karaoke::content_to_string(&line.syllables));
                if let Some(translation) = &line.translation {
                    push(translation);
                }
                if let Some(phonetic) = &line.phonetic {
                    push(phonetic);
                }
                for accompaniment in line.accompaniment_lines.iter().flatten() {
                    push(&model::karaoke::content_to_string(&accompaniment.syllables));
                    if let Some(translation) = &accompaniment.translation {
                        push(translation);
                    }
                    if let Some(phonetic) = &accompaniment.phonetic {
                        push(phonetic);
                    }
                }
            }
            SyncedLineKind::AccompanimentKaraoke(line) => {
                push(&model::karaoke::content_to_string(&line.syllables));
                if let Some(translation) = &line.translation {
                    push(translation);
                }
                if let Some(phonetic) = &line.phonetic {
                    push(phonetic);
                }
            }
        }
    }
    lines.join("\n")
}

pub fn parse_wire(content: &str) -> Vec<u8> {
    use model::SyncedLineKind;

    fn put_u8(out: &mut Vec<u8>, value: u8) {
        out.push(value);
    }
    fn put_i32(out: &mut Vec<u8>, value: i32) {
        out.extend_from_slice(&value.to_le_bytes());
    }
    fn put_string(out: &mut Vec<u8>, value: &str) {
        let bytes = value.as_bytes();
        put_i32(out, i32::try_from(bytes.len()).unwrap_or(i32::MAX));
        out.extend_from_slice(&bytes[..bytes.len().min(i32::MAX as usize)]);
    }
    fn put_optional_string(out: &mut Vec<u8>, value: Option<&str>) {
        match value {
            Some(value) => put_string(out, value),
            None => put_i32(out, -1),
        }
    }
    fn put_alignment(out: &mut Vec<u8>, value: model::KaraokeAlignment) {
        put_u8(
            out,
            match value {
                model::KaraokeAlignment::Start => 0,
                model::KaraokeAlignment::End => 1,
                model::KaraokeAlignment::Unspecified => 2,
            },
        );
    }
    fn put_syllables(out: &mut Vec<u8>, syllables: &[model::KaraokeSyllable]) {
        put_i32(out, syllables.len() as i32);
        for syllable in syllables {
            put_string(out, &syllable.content);
            put_i32(out, syllable.start);
            put_i32(out, syllable.end);
            put_optional_string(out, syllable.phonetic.as_deref());
        }
    }
    fn put_accompaniment(out: &mut Vec<u8>, line: &model::AccompanimentKaraokeLine) {
        put_syllables(out, &line.syllables);
        put_optional_string(out, line.translation.as_deref());
        put_alignment(out, line.alignment);
        put_i32(out, line.start);
        put_i32(out, line.end);
        put_optional_string(out, line.phonetic.as_deref());
    }

    let lyrics = parse_auto(content);
    let mut out = Vec::with_capacity(content.len().min(256 * 1024));
    out.extend_from_slice(b"LYR1");
    put_string(&mut out, &lyrics.title);
    put_string(&mut out, &lyrics.id);
    put_i32(&mut out, lyrics.artists.len() as i32);
    for artist in &lyrics.artists {
        put_string(&mut out, &artist.kind);
        put_string(&mut out, &artist.name);
    }
    put_i32(&mut out, lyrics.lines.len() as i32);
    for line in &lyrics.lines {
        match line {
            SyncedLineKind::Synced(line) => {
                put_u8(&mut out, 0);
                put_string(&mut out, &line.content);
                put_optional_string(&mut out, line.translation.as_deref());
                put_i32(&mut out, line.start);
                put_i32(&mut out, line.end);
            }
            SyncedLineKind::MainKaraoke(line) => {
                put_u8(&mut out, 1);
                put_syllables(&mut out, &line.syllables);
                put_optional_string(&mut out, line.translation.as_deref());
                put_alignment(&mut out, line.alignment);
                put_i32(&mut out, line.start);
                put_i32(&mut out, line.end);
                put_optional_string(&mut out, line.phonetic.as_deref());
                let accompaniment = line.accompaniment_lines.as_deref().unwrap_or_default();
                put_i32(&mut out, accompaniment.len() as i32);
                for line in accompaniment {
                    put_accompaniment(&mut out, line);
                }
            }
            SyncedLineKind::AccompanimentKaraoke(line) => {
                put_u8(&mut out, 2);
                put_accompaniment(&mut out, line);
            }
        }
    }
    // Keep a normalized pure-text trailer in the same allocation. Android can
    // write this ByteBuffer slice straight to its search index without parsing
    // twice or constructing an intermediate Java String/ByteArray.
    put_string(&mut out, &plain_text_from_lyrics(&lyrics).to_lowercase());
    out
}

pub fn parse_lrc(content: &str) -> model::SyncedLyrics {
    EnhancedLrcParser.parse(content)
}

pub fn parse_lrc_file(path: &std::path::Path) -> Result<model::SyncedLyrics, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    Ok(parse_auto(&content))
}

pub use renderer_wire::SceneBuildParams;

pub fn scene_json(lyrics: &model::SyncedLyrics, width: u32, height: u32) -> String {
    renderer_wire::scene_json(lyrics, width, height)
}

pub fn scene_json_with(lyrics: &model::SyncedLyrics, params: &SceneBuildParams) -> String {
    renderer_wire::scene_json_with(lyrics, params)
}
