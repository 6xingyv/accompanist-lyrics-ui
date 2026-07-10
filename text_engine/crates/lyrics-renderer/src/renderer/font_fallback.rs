use cosmic_text::{fontdb, Fallback, FontSystem};
use unicode_script::Script;

const CJK_FALLBACK_SC: &[&str] = &[
    // Keep the desktop/default tower sans-serif.  Windows ships the UI faces
    // below, but cosmic-text does not know that "sans-serif" should resolve to
    // them and used to skip straight to an installed Source Han *Serif* face.
    "Microsoft YaHei UI",
    "Microsoft YaHei",
    "DengXian",
    "SimHei",
    "Noto Sans CJK SC",
    "Noto Sans SC",
    "Source Han Sans SC",
    "Droid Sans Fallback",
];
const CJK_FALLBACK_TC: &[&str] = &[
    "Microsoft JhengHei UI",
    "Microsoft JhengHei",
    "Noto Sans CJK TC",
    "Noto Sans CJK HK",
    "Noto Sans TC",
    "Source Han Sans TC",
    "Droid Sans Fallback",
];
const CJK_FALLBACK_JP: &[&str] = &[
    "Yu Gothic UI",
    "Yu Gothic",
    "Meiryo UI",
    "Meiryo",
    "Noto Sans CJK JP",
    "Noto Sans JP",
    "Source Han Sans JP",
    "Droid Sans Fallback",
];
const CJK_FALLBACK_KR: &[&str] = &[
    "Malgun Gothic",
    "Noto Sans CJK KR",
    "Noto Sans KR",
    "Source Han Sans KR",
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

    let regional_priority = if locale.starts_with("ja") {
        match (is_jp, is_sc, is_tc || is_hk, is_kr) {
            (true, _, _, _) => 0,
            (_, true, _, _) => 1,
            (_, _, true, _) => 2,
            (_, _, _, true) => 3,
            _ => 4,
        }
    } else if locale.starts_with("ko") {
        match (is_kr, is_sc, is_tc || is_hk, is_jp) {
            (true, _, _, _) => 0,
            (_, true, _, _) => 1,
            (_, _, true, _) => 2,
            (_, _, _, true) => 3,
            _ => 4,
        }
    } else if locale.starts_with("zh-hant")
        || locale.starts_with("zh-tw")
        || locale.starts_with("zh-hk")
        || locale.starts_with("zh-mo")
    {
        match (is_tc || is_hk, is_sc, is_jp, is_kr) {
            (true, _, _, _) => 0,
            (_, true, _, _) => 1,
            (_, _, true, _) => 2,
            (_, _, _, true) => 3,
            _ => 4,
        }
    } else {
        match (is_sc, is_tc || is_hk, is_jp, is_kr) {
            (true, _, _, _) => 0,
            (_, true, _, _) => 1,
            (_, _, true, _) => 2,
            (_, _, _, true) => 3,
            _ => 4,
        }
    };

    // Within the right regional variant, a UI/sans face must outrank a serif
    // face. The previous score treated "Source Han Sans SC" and
    // "Source Han Serif SC" as identical and let insertion order decide.
    let serif_penalty = usize::from(family.contains("serif") || family.contains("song"));
    regional_priority * 2 + serif_penalty
}

fn family_contains_any(family: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| family.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_cjk_fallbacks_prefer_windows_ui_sans_faces() {
        assert_eq!(cjk_fallback_for_locale("zh-CN")[0], "Microsoft YaHei UI");
        assert_eq!(cjk_fallback_for_locale("zh-TW")[0], "Microsoft JhengHei UI");
        assert_eq!(cjk_fallback_for_locale("ja-JP")[0], "Yu Gothic UI");
        assert_eq!(cjk_fallback_for_locale("ko-KR")[0], "Malgun Gothic");
        for family in CJK_FALLBACK_SC
            .iter()
            .chain(CJK_FALLBACK_TC)
            .chain(CJK_FALLBACK_JP)
            .chain(CJK_FALLBACK_KR)
        {
            assert!(!family.to_ascii_lowercase().contains("serif"));
        }
    }

    #[test]
    fn cjk_priority_prefers_sans_over_serif_in_the_same_region() {
        assert!(
            cjk_family_priority("Source Han Sans SC", "zh-CN")
                < cjk_family_priority("Source Han Serif SC", "zh-CN")
        );
    }
}
