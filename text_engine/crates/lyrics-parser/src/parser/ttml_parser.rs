use super::lyrics_parser::LyricsParser;
use crate::model::{
    karaoke::{strip_enclosing_parentheses, strip_enclosing_parentheses_text},
    AccompanimentKaraokeLine, KaraokeAlignment, KaraokeSyllable, MainKaraokeLine, SyncedLine,
    SyncedLineKind, SyncedLyrics,
};
use crate::utils::simple_xml_parser::{SimpleXmlParser, XmlElement};
use crate::utils::time_utils::parse_as_time;
use std::collections::HashMap;

#[derive(Default)]
pub struct TtmlParser;

#[derive(Clone, Debug, Default)]
struct TtmlTranslation {
    main: Option<String>,
    background: Option<String>,
}

impl LyricsParser for TtmlParser {
    fn can_parse(&self, content: &str) -> bool {
        content.contains("http://www.w3.org/ns/ttml")
    }

    fn parse(&self, content: &str) -> SyncedLyrics {
        let root = SimpleXmlParser.parse(&preformatting_ttml(content));
        let metadata = find_metadata(&root);
        let agent_types = metadata.map(parse_agent_types).unwrap_or_default();
        let translations = parse_itunes_translations(&root);
        let transliterations = parse_itunes_transliterations(&root);

        let mut sorted_p = find_all_p_elements(&root)
            .into_iter()
            .map(|element| {
                let begin = element.attr("begin").map(parse_as_time).unwrap_or(i32::MAX);
                (element, begin)
            })
            .collect::<Vec<_>>();
        sorted_p.sort_by_key(|(_, begin)| *begin);
        let sorted_p = sorted_p
            .into_iter()
            .map(|(element, _)| element)
            .collect::<Vec<_>>();
        let line_alignments = compute_line_alignments(&sorted_p, &agent_types);

        let mut parsed_lines = sorted_p
            .iter()
            .zip(line_alignments)
            .filter_map(|(p, alignment)| {
                parse_single_line(p, alignment, &translations, &transliterations)
            })
            .collect::<Vec<_>>();
        parsed_lines.sort_by_key(|line| line.start());
        SyncedLyrics::new(parsed_lines)
    }
}

fn preformatting_ttml(content: &str) -> String {
    content
        .replace(" </span><span", "</span> <span")
        .replace(",</span><span", ",</span> <span")
}

fn decode_xml_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
}

fn normalize_xml_text_content(text: &str) -> String {
    decode_xml_entities(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn parse_single_line(
    p: &XmlElement,
    alignment: KaraokeAlignment,
    translations: &HashMap<String, TtmlTranslation>,
    transliterations: &HashMap<String, Vec<String>>,
) -> Option<SyncedLineKind> {
    let start = p.attr("begin").map(parse_as_time)?;
    let end = p.attr("end").map(parse_as_time)?;
    let itunes_key = p.attr_any(&["itunes:key", "key"]);

    let mut syllables = parse_syllables_from_children(&p.children);
    if let Some(phonetics) = itunes_key.and_then(|key| transliterations.get(key)) {
        if phonetics.len() == syllables.len() {
            for (syllable, phonetic) in syllables.iter_mut().zip(phonetics) {
                syllable.phonetic = Some(phonetic.clone());
            }
        }
    }

    let line_phonetic = p
        .children
        .iter()
        .find(|child| child.name == "span" && child.has_role("x-roman"))
        .map(|child| child.text.trim().to_string())
        .filter(|value| !value.is_empty());
    let inline_translation = p
        .children
        .iter()
        .find(|child| {
            child.name == "span" && child.has_role("x-translation") && !child.has_role("x-bg")
        })
        .map(|child| child.text.trim().to_string())
        .filter(|value| !value.is_empty());
    let itunes_translation = itunes_key.and_then(|key| translations.get(key));

    let accompaniment_lines = p
        .children
        .iter()
        .filter(|child| child.name == "span" && child.has_role("x-bg"))
        .filter_map(|bg| parse_accompaniment(bg, itunes_key, Some(alignment), translations))
        .collect::<Vec<_>>();

    if syllables.is_empty() && accompaniment_lines.is_empty() {
        let content = normalize_xml_text_content(&extract_all_text(p));
        if content.is_empty() {
            return None;
        }
        return Some(
            SyncedLine::new(
                content,
                inline_translation.or_else(|| itunes_translation.and_then(|t| t.main.clone())),
                start,
                end,
            )
            .into(),
        );
    }

    let mut line = MainKaraokeLine::new(
        syllables,
        inline_translation.or_else(|| itunes_translation.and_then(|t| t.main.clone())),
        alignment,
        start,
        end,
    );
    line.phonetic = line_phonetic;
    line.accompaniment_lines = (!accompaniment_lines.is_empty()).then_some(accompaniment_lines);
    Some(line.into())
}

fn parse_accompaniment(
    bg_span: &XmlElement,
    parent_key: Option<&str>,
    alignment: Option<KaraokeAlignment>,
    translations: &HashMap<String, TtmlTranslation>,
) -> Option<AccompanimentKaraokeLine> {
    let syllables = strip_enclosing_parentheses(parse_syllables_from_children(&bg_span.children));
    if syllables.is_empty() {
        return None;
    }
    let bg_key = bg_span.attr_any(&["itunes:key", "key"]).or(parent_key);
    let bg_translation = bg_span
        .children
        .iter()
        .find(|child| child.has_role("x-translation"))
        .map(extract_text_content)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| bg_key.and_then(|key| translations.get(key).and_then(|t| t.background.clone())))
        .map(|value| strip_enclosing_parentheses_text(&value));

    Some(AccompanimentKaraokeLine::new(
        syllables.clone(),
        bg_translation,
        alignment.unwrap_or(KaraokeAlignment::Start),
        bg_span
            .attr("begin")
            .map(parse_as_time)
            .unwrap_or_else(|| syllables.first().unwrap().start),
        bg_span
            .attr("end")
            .map(parse_as_time)
            .unwrap_or_else(|| syllables.last().unwrap().end),
    ))
}

fn parse_syllables_from_children(children: &[XmlElement]) -> Vec<KaraokeSyllable> {
    let mut syllables = Vec::new();
    for (index, child) in children.iter().enumerate() {
        if child.name != "span" {
            continue;
        }
        let mut span_begin = None;
        let mut span_end = None;
        let mut excluded_role = false;
        for attr in &child.attributes {
            match attr.name.as_str() {
                "begin" => span_begin = Some(attr.value.as_str()),
                "end" => span_end = Some(attr.value.as_str()),
                name if name.ends_with(":role")
                    && (attr.value == "x-translation" || attr.value == "x-bg") =>
                {
                    excluded_role = true
                }
                _ => {}
            }
        }
        if excluded_role || span_begin.is_none() || span_end.is_none() || child.text.is_empty() {
            continue;
        }

        let mut syllable_content = decode_xml_entities(&child.text);
        if let Some(next) = children.get(index + 1) {
            if next.name == "#text" {
                syllable_content.push_str(&decode_xml_entities(&next.text));
            }
        }
        syllables.push(KaraokeSyllable::new(
            syllable_content,
            parse_as_time(span_begin.unwrap()),
            parse_as_time(span_end.unwrap()),
        ));
    }

    if let Some(last) = syllables.last_mut() {
        last.content = last.content.trim_end().to_string();
    }
    syllables
}

fn parse_itunes_translations(element: &XmlElement) -> HashMap<String, TtmlTranslation> {
    let mut translations = HashMap::new();
    fn visit(element: &XmlElement, translations: &mut HashMap<String, TtmlTranslation>) {
        if element.name == "translation" || element.name.ends_with(":translation") {
            for text_elem in &element.children {
                if text_elem.name != "text" {
                    continue;
                }
                let Some(key) = text_elem.attr("for") else {
                    continue;
                };
                let main = decode_xml_entities(&text_elem.text).trim().to_string();
                let main = (!main.is_empty()).then_some(main);
                let background = text_elem
                    .children
                    .iter()
                    .filter(|child| child.name == "span" && child.has_role("x-bg"))
                    .map(extract_text_content)
                    .map(|value| decode_xml_entities(&value).trim().to_string())
                    .filter(|value| !value.is_empty())
                    .collect::<String>();
                let background = (!background.is_empty()).then_some(background);
                if main.is_some() || background.is_some() {
                    translations.insert(key.to_string(), TtmlTranslation { main, background });
                }
            }
        }
        for child in &element.children {
            visit(child, translations);
        }
    }
    visit(element, &mut translations);
    translations
}

fn parse_itunes_transliterations(element: &XmlElement) -> HashMap<String, Vec<String>> {
    let mut transliterations = HashMap::new();
    fn visit(element: &XmlElement, transliterations: &mut HashMap<String, Vec<String>>) {
        if element.name == "transliterations" || element.name.ends_with(":transliterations") {
            for trans_elem in &element.children {
                if !(trans_elem.name == "transliteration"
                    || trans_elem.name.ends_with(":transliteration"))
                {
                    continue;
                }
                for text_elem in &trans_elem.children {
                    if text_elem.name != "text" {
                        continue;
                    }
                    let Some(key) = text_elem.attr("for") else {
                        continue;
                    };
                    let phonetic_spans = text_elem
                        .children
                        .iter()
                        .filter(|child| child.name == "span")
                        .map(|child| decode_xml_entities(&child.text).trim().to_string())
                        .collect::<Vec<_>>();
                    if !phonetic_spans.is_empty() {
                        transliterations.insert(key.to_string(), phonetic_spans);
                    }
                }
            }
        }
        for child in &element.children {
            visit(child, transliterations);
        }
    }
    visit(element, &mut transliterations);
    transliterations
}

fn extract_all_text(element: &XmlElement) -> String {
    let mut result = element.text.clone();
    for child in &element.children {
        if child.name == "span"
            && (child.has_role("x-translation")
                || child.has_role("x-bg")
                || child.has_role("x-roman"))
        {
            continue;
        }
        result.push_str(&extract_all_text(child));
    }
    result
}

fn extract_text_content(element: &XmlElement) -> String {
    let mut result = element.text.clone();
    for child in &element.children {
        result.push_str(&extract_text_content(child));
    }
    result
}

fn find_metadata(element: &XmlElement) -> Option<&XmlElement> {
    if element.name == "metadata" {
        return Some(element);
    }
    element.children.iter().find_map(find_metadata)
}

fn parse_agent_types(metadata: &XmlElement) -> HashMap<String, String> {
    metadata
        .children
        .iter()
        .filter(|child| child.name.ends_with(":agent") || child.name == "agent")
        .filter_map(|agent| {
            let id = agent.attr_any(&["xml:id", "id"])?;
            let kind = agent.attr("type").unwrap_or("person");
            Some((id.to_string(), kind.to_string()))
        })
        .collect()
}

fn compute_line_alignments(
    sorted_lines: &[&XmlElement],
    agent_types: &HashMap<String, String>,
) -> Vec<KaraokeAlignment> {
    let first_ordinal = sorted_lines
        .first()
        .and_then(|line| line.attr("ttm:agent"))
        .map(|agent| {
            agent
                .chars()
                .rev()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        })
        .and_then(|digits| digits.parse::<i32>().ok())
        .unwrap_or(1);
    let mut on_right = first_ordinal % 2 == 0;
    let mut current_person: Option<String> = None;

    sorted_lines
        .iter()
        .map(|p| {
            let agent_id = p.attr("ttm:agent");
            let agent_type = agent_id
                .and_then(|id| agent_types.get(id))
                .map(|value| value.as_str())
                .unwrap_or(if agent_id.is_some() { "person" } else { "none" });
            match agent_type {
                "group" => KaraokeAlignment::Start,
                "other" => KaraokeAlignment::End,
                "person" => {
                    if current_person.is_none() {
                        current_person = agent_id.map(ToString::to_string);
                    } else if agent_id != current_person.as_deref() {
                        on_right = !on_right;
                        current_person = agent_id.map(ToString::to_string);
                    }
                    if on_right {
                        KaraokeAlignment::End
                    } else {
                        KaraokeAlignment::Start
                    }
                }
                _ => {
                    if on_right {
                        KaraokeAlignment::End
                    } else {
                        KaraokeAlignment::Start
                    }
                }
            }
        })
        .collect()
}

fn find_all_p_elements(element: &XmlElement) -> Vec<&XmlElement> {
    let mut result = Vec::new();
    if element.name == "p" {
        result.push(element);
    }
    for child in &element.children {
        result.extend(find_all_p_elements(child));
    }
    result
}
