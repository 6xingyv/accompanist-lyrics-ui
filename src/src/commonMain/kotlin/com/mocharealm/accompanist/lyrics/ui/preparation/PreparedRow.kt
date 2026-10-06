package com.mocharealm.accompanist.lyrics.ui.preparation

import androidx.compose.ui.geometry.Rect

class PreparedRow
internal constructor(
    val runs: List<PreparedProfileRun>,
    val bounds: Rect,
    val rtl: Boolean,
    val start: Int,
    val end: Int,
    val sweepEnd: Int,
    val animated: Boolean,
    internal val windows: List<RenderWindow>,
    val sweepStarts: IntArray,
    val sweepEnds: IntArray,
    val sweepLeft: FloatArray,
    val sweepRight: FloatArray,
    val top: Float,
    val height: Float,
    val phoneticHeight: Float,
    val sweepFadeWidth: Float,
    /** Extra space after the preceding wrapped row's inline phonetics. */
    val phoneticSpacingBefore: Float = 0f,
)
