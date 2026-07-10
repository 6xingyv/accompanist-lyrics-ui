use base64::Engine;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    pub translations: Vec<String>,
    pub phonetics: Vec<Vec<String>>,
}

pub fn decode(language_header: Option<&str>) -> Metadata {
    let Some(language_header) = language_header.filter(|value| !value.trim().is_empty()) else {
        return Metadata::default();
    };
    let content_base64 = language_header
        .trim()
        .strip_prefix("[language:")
        .unwrap_or(language_header)
        .strip_suffix(']')
        .unwrap_or(language_header)
        .trim();
    if content_base64.is_empty() {
        return Metadata::default();
    }

    let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(content_base64) else {
        return Metadata::default();
    };
    let Ok(json) = String::from_utf8(decoded) else {
        return Metadata::default();
    };
    parse_json_content(&json)
}

fn parse_json_content(json: &str) -> Metadata {
    let Ok(root) = serde_json::from_str::<serde_json::Value>(json) else {
        return Metadata::default();
    };
    let Some(content_array) = root.get("content").and_then(|value| value.as_array()) else {
        return Metadata::default();
    };

    let mut lyric_lines = Vec::new();
    let mut pron_lines = Vec::new();

    for element in content_array {
        let item_type = element.get("type").and_then(|value| value.as_i64());
        let Some(all_rows) = element
            .get("lyricContent")
            .and_then(|value| value.as_array())
        else {
            continue;
        };
        if item_type == Some(1) {
            for row in all_rows {
                let Some(row) = row.as_array() else {
                    continue;
                };
                let full_line = row
                    .iter()
                    .filter_map(|part| part.as_str())
                    .collect::<String>();
                lyric_lines.push(full_line);
            }
        } else if item_type == Some(0) {
            for row in all_rows {
                let Some(row) = row.as_array() else {
                    continue;
                };
                let row_syllables = row
                    .iter()
                    .filter_map(|syllable_parts| syllable_parts.as_array())
                    .map(|parts| parts.iter().filter_map(|part| part.as_str()).collect())
                    .collect::<Vec<String>>();
                pron_lines.push(row_syllables);
            }
        }
    }

    Metadata {
        translations: lyric_lines,
        phonetics: pron_lines,
    }
}
