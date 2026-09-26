package com.mocharealm.accompanist.lyrics.ui.internal.text

import com.mocharealm.accompanist.lyrics.ui.internal.text.*
import com.mocharealm.accompanist.lyrics.ui.profile.*

import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import kotlin.test.*

class BuiltinProfileDispatchTest {
    @Test
    fun dispatchPreservesAllBmpMatchesAndPriority() {
        val orders =
            listOf(
                DefaultLyricsProfiles,
                DefaultLyricsProfiles.reversed(),
                DefaultLyricsProfiles.filter { it !== HangulJamoProfile },
                listOf(CjkProfile, HangulJamoProfile, LatinProfile),
                emptyList(),
            )
        for (profiles in orders) {
            val dispatch = BuiltinProfileDispatch(profiles)
            for (codePoint in 0..0xFFFF) {
                val syllable = KaraokeSyllable(codePoint.toChar().toString(), 0, 100)
                val expected = profiles.firstOrNull { it.matches(syllable) } ?: FallbackProfile
                assertSame(expected, dispatch.profile(codePoint), "U+${codePoint.toString(16)}")
            }
            for (codePoint in listOf(0x20000, 0x323AF, 0x1F600, 0x10000)) {
                val text = String(Character.toChars(codePoint))
                val expected =
                    profiles.firstOrNull { it.matches(KaraokeSyllable(text, 0, 100)) }
                        ?: FallbackProfile
                assertSame(expected, dispatch.profile(codePoint))
            }
        }
    }

    @Test
    fun singlePassMatchesLegacyGroupingTimingAndWhitespace() {
        val never =
            object : WordLevelLyricsProfile() {
                override fun matches(syllable: KaraokeSyllable) = false
            }
        val sources =
            listOf(
                    "",
                    "  ",
                    "Ahhh This is 我的生活 不安です",
                    "שלום, Привет!",
                    "한 العربية",
                    "🙂𠀀 hello",
                    "नमस्ते ภาษาไทย",
                )
                .mapIndexed { index, text ->
                    KaraokeSyllable(text, index * 3000, (index + 1) * 3000)
                }
        for (leading in listOf(false, true)) {
            val expected =
                resolveProfiles(sources, listOf(never) + DefaultLyricsProfiles, leading)
            val actual = resolveProfiles(sources, DefaultLyricsProfiles, leading)
            assertEquals(expected, actual)
        }
    }
}
