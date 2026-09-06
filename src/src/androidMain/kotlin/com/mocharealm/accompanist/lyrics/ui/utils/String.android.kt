package com.mocharealm.accompanist.lyrics.ui.utils

import android.os.Build

private val cjkBlocks: Set<Character.UnicodeBlock> by lazy {
    mutableSetOf(
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
        Character.UnicodeBlock.HANGUL_COMPATIBILITY_JAMO
    ).apply {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            add(Character.UnicodeBlock.CJK_UNIFIED_IDEOGRAPHS_EXTENSION_E)
            add(Character.UnicodeBlock.CJK_UNIFIED_IDEOGRAPHS_EXTENSION_F)
            add(Character.UnicodeBlock.CJK_UNIFIED_IDEOGRAPHS_EXTENSION_G)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.BAKLAVA) {
            add(Character.UnicodeBlock.CJK_UNIFIED_IDEOGRAPHS_EXTENSION_H)
        }
    }
}

private val arabicBlocks: Set<Character.UnicodeBlock> by lazy {
    mutableSetOf(
        Character.UnicodeBlock.ARABIC,
        Character.UnicodeBlock.ARABIC_SUPPLEMENT,
        Character.UnicodeBlock.ARABIC_EXTENDED_A,
        Character.UnicodeBlock.ARABIC_PRESENTATION_FORMS_A,
        Character.UnicodeBlock.ARABIC_PRESENTATION_FORMS_B
    ).apply {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.BAKLAVA) {
            add(Character.UnicodeBlock.ARABIC_EXTENDED_B)
        }
    }
}

private val indicBlocks: Set<Character.UnicodeBlock> by lazy {
    setOf(
        Character.UnicodeBlock.DEVANAGARI,
        Character.UnicodeBlock.DEVANAGARI_EXTENDED,
        Character.UnicodeBlock.BENGALI,
        Character.UnicodeBlock.GURMUKHI,
        Character.UnicodeBlock.GUJARATI,
        Character.UnicodeBlock.ORIYA,
        Character.UnicodeBlock.TAMIL,
        Character.UnicodeBlock.TELUGU,
        Character.UnicodeBlock.KANNADA,
        Character.UnicodeBlock.MALAYALAM,
        Character.UnicodeBlock.SINHALA,
        Character.UnicodeBlock.VEDIC_EXTENSIONS
    )
}

actual fun Char.isCjk(): Boolean {
    return try {
        Character.UnicodeBlock.of(this) in cjkBlocks
    } catch (e: Exception) {
        false
    }
}

actual fun Char.isArabic(): Boolean {
    return try {
        Character.UnicodeBlock.of(this) in arabicBlocks
    } catch (e: Exception) {
        false
    }
}

actual fun Char.isIndic(): Boolean {
    return try {
        Character.UnicodeBlock.of(this) in indicBlocks
    } catch (e: Exception) {
        false
    }
}