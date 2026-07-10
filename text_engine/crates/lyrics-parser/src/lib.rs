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
