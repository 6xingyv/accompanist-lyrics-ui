package com.mocharealm.accompanist.lyrics.ui.utils

private val cjkBlock = listOf(
    Character.UnicodeBlock.CJK_UNIFIED_IDEOGRAPHS,
    Character.UnicodeBlock.CJK_UNIFIED_IDEOGRAPHS_EXTENSION_A,
    Character.UnicodeBlock.CJK_UNIFIED_IDEOGRAPHS_EXTENSION_B,
    Character.UnicodeBlock.CJK_UNIFIED_IDEOGRAPHS_EXTENSION_C,
    Character.UnicodeBlock.CJK_UNIFIED_IDEOGRAPHS_EXTENSION_D,
    Character.UnicodeBlock.CJK_COMPATIBILITY_IDEOGRAPHS,
    Character.UnicodeBlock.CJK_SYMBOLS_AND_PUNCTUATION,
    Character.UnicodeBlock.HIRAGANA,
    Character.UnicodeBlock.KATAKANA,
    Character.UnicodeBlock.HANGUL_SYLLABLES,
    Character.UnicodeBlock.HANGUL_JAMO,
    Character.UnicodeBlock.HANGUL_COMPATIBILITY_JAMO,
    Character.UnicodeBlock.CJK_UNIFIED_IDEOGRAPHS_EXTENSION_E,
    Character.UnicodeBlock.CJK_UNIFIED_IDEOGRAPHS_EXTENSION_F,
    Character.UnicodeBlock.CJK_UNIFIED_IDEOGRAPHS_EXTENSION_G,
    Character.UnicodeBlock.CJK_UNIFIED_IDEOGRAPHS_EXTENSION_H
)

actual fun Char.isCjk(): Boolean {
    return Character.UnicodeBlock.of(this) in cjkBlock
}