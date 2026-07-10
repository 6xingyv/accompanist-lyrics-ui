use crate::model::Attributes;
use regex::Regex;
use std::sync::OnceLock;

const METADATA_TAGS: &[&str] = &["ar", "ti", "al", "offset", "length"];

fn attribute_parser() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^\[([a-zA-Z]+):\s*(.*)\]\s*$").unwrap())
}

pub fn parse(lines: &[String]) -> Attributes {
    let mut attributes = Attributes::default();
    for line in lines {
        let Some(captures) = attribute_parser().captures(line) else {
            continue;
        };
        let tag = captures.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        if !METADATA_TAGS.contains(&tag) {
            continue;
        }
        let value = captures.get(2).map(|m| m.as_str().trim()).unwrap_or("");
        match tag {
            "ar" => attributes.artist = Some(value.to_string()),
            "ti" => attributes.title = Some(value.to_string()),
            "al" => attributes.album = Some(value.to_string()),
            "offset" => attributes.offset = value.parse().unwrap_or(0),
            "length" => attributes.duration = value.parse().unwrap_or(0),
            _ => {}
        }
    }
    attributes
}

pub fn remove_attributes(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|line| {
            let Some(captures) = attribute_parser().captures(line.as_str()) else {
                return true;
            };
            let tag = captures.get(1).map(|m| m.as_str()).unwrap_or("");
            !METADATA_TAGS.contains(&tag)
        })
        .cloned()
        .collect()
}
