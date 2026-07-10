pub mod enhanced_lrc_exporter;
pub mod lrc_exporter;
pub mod ttml_exporter;

pub trait LyricsExporter {
    fn export(&self, lyrics: &crate::model::SyncedLyrics) -> String;
}
