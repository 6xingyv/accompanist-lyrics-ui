package com.mocharealm.accompanist.lyrics.ui.preparation

import androidx.compose.ui.text.TextLayoutResult
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine

class PreparedLine
internal constructor(
    val source: KaraokeLine,
    val runs: List<PreparedProfileRun>,
    val rows: List<PreparedRow>,
    val width: Float,
    val height: Float,
    val rightAligned: Boolean,
    val before: List<PreparedLine>,
    val after: List<PreparedLine>,
    val translation: TextLayoutResult?,
    val phonetic: TextLayoutResult?,
    /**
     * The same shaping laid out without reserving caption slots. Present only when a caption is
     * wider than its word, because that is the only case where hiding pronunciation must change
     * word positions or row breaks.
     */
    internal val compact: PreparedLine? = null,
    /** Unit correspondence between this geometry and [compact]. */
    internal val binding: LineVariantBinding? = null,
) {
    internal var visibilityStart: Int = source.start
        private set

    internal var visibilityEnd: Int = source.end
        private set

    internal var revealFromBottom: Boolean = false
        private set

    internal fun variant(alignedPhonetics: Boolean): PreparedLine =
        if (alignedPhonetics) this else compact ?: this

    init {
        for (child in before + after) {
            child.visibilityStart = minOf(child.source.start, source.start)
            child.visibilityEnd = maxOf(child.source.end, source.end)
            child.revealFromBottom = child.source.start < source.start
            // Variants share their source interval, so they share its visibility window too.
            child.compact?.let { compactChild ->
                compactChild.visibilityStart = child.visibilityStart
                compactChild.visibilityEnd = child.visibilityEnd
                compactChild.revealFromBottom = child.revealFromBottom
            }
        }
    }
}
