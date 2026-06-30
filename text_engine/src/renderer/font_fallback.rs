use cosmic_text::{fontdb, Fallback, FontSystem};
use unicode_script::Script;

const CJK_FALLBACK_SC: &[&str] = &[
    "Noto Sans CJK SC",
    "Noto Serif CJK SC",
    "Noto Sans SC",
    "Noto Serif SC",
    "Source Han Sans SC",
    "Source Han Serif SC",
    "Droid Sans Fallback",
];
const CJK_FALLBACK_TC: &[&str] = &[
    "Noto Sans CJK TC",
    "Noto Serif CJK TC",
    "Noto Sans CJK HK",
    "Noto Serif CJK HK",
    "Noto Sans TC",
    "Noto Serif TC",
    "Source Han Sans TC",
    "Source Han Serif TC",
    "Droid Sans Fallback",
];
const CJK_FALLBACK_JP: &[&str] = &[
    "Noto Sans CJK JP",
    "Noto Serif CJK JP",
    "Noto Sans JP",
    "Noto Serif JP",
    "Source Han Sans JP",
    "Source Han Serif JP",
    "Droid Sans Fallback",
];
const CJK_FALLBACK_KR: &[&str] = &[
    "Noto Sans CJK KR",
    "Noto Serif CJK KR",
    "Noto Sans KR",
    "Noto Serif KR",
    "Source Han Sans KR",
    "Source Han Serif KR",
    "Droid Sans Fallback",
];

#[derive(Debug)]
struct CjkFallback;

impl Fallback for CjkFallback {
    fn common_fallback(&self) -> &[&'static str] {
        &[]
    }

    fn forbidden_fallback(&self) -> &[&'static str] {
        &[]
    }

    fn script_fallback(&self, script: Script, locale: &str) -> &[&'static str] {
        match script {
            Script::Han => cjk_fallback_for_locale(locale),
            Script::Hiragana | Script::Katakana => CJK_FALLBACK_JP,
            Script::Hangul => CJK_FALLBACK_KR,
            Script::Bopomofo => CJK_FALLBACK_TC,
            _ => &[],
        }
    }
}

pub(super) fn new_font_system(locale: String, db: fontdb::Database) -> FontSystem {
    FontSystem::new_with_locale_and_db_and_fallback(locale, db, CjkFallback)
}

fn cjk_fallback_for_locale(locale: &str) -> &'static [&'static str] {
    let locale = locale.to_ascii_lowercase();
    if locale.starts_with("ja") {
        CJK_FALLBACK_JP
    } else if locale.starts_with("ko") {
        CJK_FALLBACK_KR
    } else if locale.starts_with("zh-hant")
        || locale.starts_with("zh-tw")
        || locale.starts_with("zh-hk")
        || locale.starts_with("zh-mo")
    {
        CJK_FALLBACK_TC
    } else {
        CJK_FALLBACK_SC
    }
}

pub(super) fn cjk_family_priority(family_name: &str, locale: &str) -> usize {
    let locale = locale.to_ascii_lowercase();
    let family = family_name.to_ascii_lowercase();
    let is_sc = family_contains_any(&family, &["cjk sc", "sans sc", "serif sc", "hans"]);
    let is_tc = family_contains_any(&family, &["cjk tc", "sans tc", "serif tc", "hant"]);
    let is_hk = family_contains_any(&family, &["cjk hk", "sans hk", "serif hk"]);
    let is_jp = family_contains_any(&family, &["cjk jp", "sans jp", "serif jp", "japanese"]);
    let is_kr = family_contains_any(&family, &["cjk kr", "sans kr", "serif kr", "korean"]);

    if locale.starts_with("ja") {
        return match (is_jp, is_sc, is_tc || is_hk, is_kr) {
            (true, _, _, _) => 0,
            (_, true, _, _) => 1,
            (_, _, true, _) => 2,
            (_, _, _, true) => 3,
            _ => 4,
        };
    }

    if locale.starts_with("ko") {
        return match (is_kr, is_sc, is_tc || is_hk, is_jp) {
            (true, _, _, _) => 0,
            (_, true, _, _) => 1,
            (_, _, true, _) => 2,
            (_, _, _, true) => 3,
            _ => 4,
        };
    }

    if locale.starts_with("zh-hant")
        || locale.starts_with("zh-tw")
        || locale.starts_with("zh-hk")
        || locale.starts_with("zh-mo")
    {
        return match (is_tc || is_hk, is_sc, is_jp, is_kr) {
            (true, _, _, _) => 0,
            (_, true, _, _) => 1,
            (_, _, true, _) => 2,
            (_, _, _, true) => 3,
            _ => 4,
        };
    }

    match (is_sc, is_tc || is_hk, is_jp, is_kr) {
        (true, _, _, _) => 0,
        (_, true, _, _) => 1,
        (_, _, true, _) => 2,
        (_, _, _, true) => 3,
        _ => 4,
    }
}

fn family_contains_any(family: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| family.contains(needle))
}
