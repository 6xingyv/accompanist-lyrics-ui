use super::LyricsExporter;
use crate::model::SyncedLyrics;

pub struct EnhancedLrcExporter;

impl LyricsExporter for EnhancedLrcExporter {
    fn export(&self, lyrics: &SyncedLyrics) -> String {
        crate::exporter::lrc_exporter::LrcExporter.export(lyrics)
    }
}
