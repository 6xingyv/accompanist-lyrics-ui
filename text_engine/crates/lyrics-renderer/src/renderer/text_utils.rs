pub(super) fn trailing_whitespace_count(text: &str) -> usize {
    text.chars()
        .rev()
        .take_while(|ch| ch.is_whitespace())
        .count()
}

pub(super) fn trim_end_whitespace(text: &str) -> &str {
    text.trim_end_matches(|ch: char| ch.is_whitespace())
}

pub(super) fn is_blank_text(text: &str) -> bool {
    text.chars().all(char::is_whitespace)
}

pub(super) fn contains_rtl(text: &str) -> bool {
    text.chars().any(|ch| {
        matches!(
            ch as u32,
            0x0590..=0x08ff | 0xfb1d..=0xfdff | 0xfe70..=0xfeff | 0x10800..=0x10fff
        )
    })
}

pub(super) fn contains_han(text: &str) -> bool {
    text.chars()
        .any(|ch| matches!(ch as u32, 0x3400..=0x9fff | 0xf900..=0xfaff))
}

pub(super) fn has_trailing_whitespace(text: &str) -> bool {
    text.chars().next_back().is_some_and(char::is_whitespace)
}

pub(super) fn should_use_simple_animation(text: &str) -> bool {
    let cleaned = text
        .chars()
        .filter(|ch| !ch.is_whitespace() && !is_punctuation_char(*ch))
        .collect::<Vec<_>>();
    if cleaned.is_empty() {
        return false;
    }
    cleaned.iter().all(|ch| is_cjk_char(*ch))
        || cleaned
            .iter()
            .any(|ch| is_arabic_char(*ch) || is_devanagari_char(*ch))
}

pub(super) fn is_punctuation_or_space(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|ch| ch.is_whitespace() || is_punctuation_char(ch))
}

fn is_punctuation_char(ch: char) -> bool {
    ch.is_ascii_punctuation()
        || matches!(
            ch,
            '…' | '—'
                | '–'
                | '、'
                | '。'
                | '，'
                | '！'
                | '？'
                | '；'
                | '：'
                | '“'
                | '”'
                | '‘'
                | '’'
                | '（'
                | '）'
                | '【'
                | '】'
                | '《'
                | '》'
                | '～'
                | '·'
        )
        || matches!(
            ch as u32,
            0x2000..=0x206f | 0x2e00..=0x2e7f | 0x3000..=0x303f | 0xfe10..=0xfe1f
                | 0xfe30..=0xfe4f | 0xff00..=0xff65
        )
}

fn is_cjk_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3040..=0x30ff
            | 0x3100..=0x312f
            | 0x3130..=0x318f
            | 0x3400..=0x9fff
            | 0xac00..=0xd7af
            | 0xf900..=0xfaff
            | 0x20000..=0x2ffff
            | 0x30000..=0x323af
    )
}

fn is_arabic_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0600..=0x06ff | 0x0750..=0x077f | 0x08a0..=0x08ff | 0xfb50..=0xfdff
            | 0xfe70..=0xfeff
    )
}

fn is_devanagari_char(ch: char) -> bool {
    matches!(ch as u32, 0x0900..=0x097f | 0xa8e0..=0xa8ff)
}
