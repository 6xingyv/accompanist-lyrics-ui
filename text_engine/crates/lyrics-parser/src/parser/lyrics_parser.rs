use crate::model::SyncedLyrics;

pub trait LyricsParser {
    fn can_parse(&self, content: &str) -> bool;

    fn parse_lines(&self, lines: &[String]) -> SyncedLyrics {
        self.parse(&lines.join("\n"))
    }

    fn parse(&self, content: &str) -> SyncedLyrics {
        let lines = content.lines().map(ToString::to_string).collect::<Vec<_>>();
        self.parse_lines(&lines)
    }
}
