package com.mocharealm.accompanist.lyrics.ui.internal.layout

import com.mocharealm.accompanist.lyrics.ui.preparation.PreparedLine
import com.mocharealm.accompanist.lyrics.ui.internal.playback.LyricsPlaybackState

internal fun PreparedLine.settledHeight(
    playback: LyricsPlaybackState,
    translationShown: Boolean,
    phoneticShown: Boolean,
    density: Float,
    anticipateAccompaniment: Boolean = false,
    nested: Boolean = false,
): Float {
    if (nested && !playback.line(this).visible.value && !anticipateAccompaniment) return 0f
    // Hidden pronunciation falls back to the compact geometry, so heights follow its rows too.
    val geometry = variant(phoneticShown)
    var extent = geometry.height + if (nested) 16f * density else 0f
    if (!phoneticShown) for (row in geometry.rows)
        extent -= row.phoneticHeight + row.phoneticSpacingBefore
    if (translationShown) extent += translation?.size?.height ?: 0
    if (phoneticShown) extent += phonetic?.size?.height ?: 0
    if ((translationShown && translation != null) || (phoneticShown && phonetic != null))
        extent += 8f * density
    for (line in before) extent +=
        line.settledHeight(playback, translationShown, phoneticShown, density, anticipateAccompaniment, true)
    for (line in after) extent +=
        line.settledHeight(playback, translationShown, phoneticShown, density, anticipateAccompaniment, true)
    return extent
}
