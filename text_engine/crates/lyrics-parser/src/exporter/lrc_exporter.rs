use super::LyricsExporter;
use crate::model::{SyncedLineKind, SyncedLyrics};
use crate::utils::time_utils::to_time_formatted_string;

pub struct LrcExporter;

impl LyricsExporter for LrcExporter {
    fn export(&self, lyrics: &SyncedLyrics) -> String {
        let mut result = String::new();
        if !lyrics.title.is_empty() {
            result.push_str(&format!("[ti:{}]\n", lyrics.title));
        }
        if !lyrics.artists.is_empty() {
            result.push_str(&format!(
                "[ar:{}]\n",
                lyrics
                    .artists
                    .iter()
                    .map(|artist| artist.name.as_str())
                    .collect::<Vec<_>>()
                    .join("/")
            ));
        }
        for line in &lyrics.lines {
            let content = line.content_string();
            if content.is_empty() {
                continue;
            }
            result.push_str(&format!(
                "[{}]{}\n",
                to_time_formatted_string(line.start()),
                match line {
                    SyncedLineKind::Synced(synced) => synced.content.as_str(),
                    _ => content.as_str(),
                }
            ));
        }
        result
    }
}
