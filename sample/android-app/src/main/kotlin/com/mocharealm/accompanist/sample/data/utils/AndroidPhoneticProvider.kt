package com.mocharealm.accompanist.sample.data.utils

import android.icu.text.Transliterator
import android.os.Build
import com.github.promeg.pinyinhelper.Pinyin
import com.mocharealm.accompanist.lyrics.core.model.karaoke.PhoneticLevel
import com.mocharealm.accompanist.lyrics.core.utils.PhoneticProvider

object AndroidPhoneticProvider : PhoneticProvider {

    private val koTransliterator by lazy { Transliterator.getInstance("Hangul-Latin; Latin-ASCII") }

    private val jpTransliterator by lazy {
        Transliterator.getInstance("Hiragana-Latin; Katakana-Latin; Latin-ASCII")
    }

    private val genericTransliterator by lazy {
        Transliterator.getInstance("Any-Latin; Latin-ASCII")
    }

    override val phoneticLevel: PhoneticLevel = PhoneticLevel.SYLLABLE

    override fun getPhonetic(string: String): String {
        if (string.isBlank() || string.isPunctuation()) return string

        return when {
            string.containsKorean() -> {
                koTransliterator.transliterate(string).lowercase()
            }

            string.containsJapanese() -> {
                jpTransliterator.transliterate(string).lowercase()
            }

            string.isPureCjk() -> {
                Pinyin.toPinyin(string, " ").lowercase()
            }

            else -> {
                genericTransliterator.transliterate(string).lowercase()
            }
        }
    }
}

private fun Char.isJapaneseSample(): Boolean =
    code in 0x3040..0x309F || code in 0x30A0..0x30FF || code in 0xFF66..0xFF9F

private fun Char.isKoreanSample(): Boolean = code in 0xAC00..0xD7AF || code in 0x1100..0x11FF

private val cjkBlocksSample: Set<Character.UnicodeBlock> by lazy {
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
            Character.UnicodeBlock.HANGUL_COMPATIBILITY_JAMO,
        )
        .apply {
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

private fun Char.isCjkSample(): Boolean =
    try {
        val block = Character.UnicodeBlock.of(this) ?: return false
        block in cjkBlocksSample
    } catch (e: Exception) {
        false
    }

private fun String.containsJapanese(): Boolean = any(Char::isJapaneseSample)

private fun String.containsKorean(): Boolean = any(Char::isKoreanSample)

private fun String.isPureCjk(): Boolean {
    val cleaned = filter { it != ' ' && it != ',' && it != '\n' && it != '\r' }
    return cleaned.isNotEmpty() && cleaned.all(Char::isCjkSample)
}

private fun Char.isProfilePunctuationSample(): Boolean =
    when (category) {
        CharCategory.CONNECTOR_PUNCTUATION,
        CharCategory.DASH_PUNCTUATION,
        CharCategory.START_PUNCTUATION,
        CharCategory.END_PUNCTUATION,
        CharCategory.INITIAL_QUOTE_PUNCTUATION,
        CharCategory.FINAL_QUOTE_PUNCTUATION,
        CharCategory.OTHER_PUNCTUATION -> true
        else -> this == '～'
    }

private fun String.isPunctuation(): Boolean =
    isNotEmpty() && all { it.isWhitespace() || it.isProfilePunctuationSample() }
