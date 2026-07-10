use super::LyricsExporter;
use crate::model::SyncedLyrics;

pub struct TtmlExporter;

impl LyricsExporter for TtmlExporter {
    fn export(&self, lyrics: &SyncedLyrics) -> String {
        let body = lyrics
            .lines
            .iter()
            .map(|line| {
                format!(
                    r#"<p begin="{}ms" end="{}ms">{}</p>"#,
                    line.start(),
                    line.end(),
                    escape_xml(&line.content_string())
                )
            })
            .collect::<Vec<_>>()
            .join("");
        format!(r#"<tt xmlns="http://www.w3.org/ns/ttml"><body><div>{body}</div></body></tt>"#)
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
