package com.mocharealm.accompanist.lyrics.ui.internal.test

import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import com.mocharealm.accompanist.lyrics.ui.preparation.PreparedLine
import com.mocharealm.accompanist.lyrics.ui.preparation.prepareLyricsLine
import com.mocharealm.accompanist.lyrics.ui.profile.DefaultLyricsProfiles
import com.mocharealm.accompanist.lyrics.ui.profile.LyricsProfile

internal val measurer =
    TextMeasurer(createFontFamilyResolver(), Density(1f), LayoutDirection.Ltr)

internal val style = TextStyle(fontSize = 32.sp)

internal fun source(text: String, start: Int = 1000, end: Int = 3000) =
    KaraokeLine.MainKaraokeLine(
        listOf(KaraokeSyllable(text, start, end)),
        null,
        KaraokeAlignment.Start,
        start,
        end,
    )

internal fun prepare(
    line: KaraokeLine,
    width: Float = 500f,
    profiles: List<LyricsProfile> = DefaultLyricsProfiles,
): PreparedLine =
    prepareLyricsLine(
        line,
        profiles,
        measurer,
        style,
        style,
        TextStyle(fontSize = 12.sp),
        width,
        1f,
        true,
        translationStyle = TextStyle(fontSize = 12.sp),
    )
