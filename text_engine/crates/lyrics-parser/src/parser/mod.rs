pub mod auto_parser;
pub mod enhanced_lrc_parser;
pub mod kugou_krc_parser;
pub mod lyricify_syllable_parser;
pub mod lyrics_parser;
pub mod netease_yrc_parser;
pub mod ttml_parser;

pub use auto_parser::AutoParser;
pub use enhanced_lrc_parser::EnhancedLrcParser;
pub use kugou_krc_parser::KugouKrcParser;
pub use lyricify_syllable_parser::LyricifySyllableParser;
pub use lyrics_parser::LyricsParser;
pub use netease_yrc_parser::NeteaseYrcParser;
pub use ttml_parser::TtmlParser;
