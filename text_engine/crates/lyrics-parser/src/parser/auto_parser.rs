use super::{
    enhanced_lrc_parser::EnhancedLrcParser, kugou_krc_parser::KugouKrcParser,
    lyricify_syllable_parser::LyricifySyllableParser, lyrics_parser::LyricsParser,
    netease_yrc_parser::NeteaseYrcParser, ttml_parser::TtmlParser,
};
use crate::model::SyncedLyrics;

pub struct AutoParser {
    parsers: Vec<Box<dyn LyricsParser + Send + Sync>>,
}

impl Default for AutoParser {
    fn default() -> Self {
        Self {
            parsers: vec![
                Box::new(TtmlParser::default()),
                Box::new(LyricifySyllableParser),
                Box::new(EnhancedLrcParser),
                Box::new(KugouKrcParser),
                Box::new(NeteaseYrcParser),
            ],
        }
    }
}

impl LyricsParser for AutoParser {
    fn can_parse(&self, content: &str) -> bool {
        self.parsers.iter().any(|parser| parser.can_parse(content))
    }

    fn parse(&self, content: &str) -> SyncedLyrics {
        self.parsers
            .iter()
            .find(|parser| parser.can_parse(content))
            .map(|parser| parser.parse(content))
            .unwrap_or_else(|| SyncedLyrics::new(Vec::new()))
    }
}
