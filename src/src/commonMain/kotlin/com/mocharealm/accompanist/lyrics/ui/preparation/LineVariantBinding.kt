package com.mocharealm.accompanist.lyrics.ui.preparation

/**
 * Per-unit correspondence between the pronunciation-aligned geometry and the compact one.
 * Wrapping is shared, so units keep one global order in both variants; only their row and x
 * differ. Units that stay on the same row slide; units that change row must cross-fade.
 */
class LineVariantBinding
internal constructor(
    /** Compact x minus aligned x for every unit. */
    private val slide: FloatArray,
    private val reflow: BooleanArray,
) {
    val unitCount: Int
        get() = slide.size

    /** False when the unit changes row between variants. */
    fun slides(unit: Int): Boolean = !reflow[unit]

    /** Remaining x offset towards the compact position, given alignment progress in 0..1. */
    fun slideOffset(unit: Int, alignment: Float): Float = slide[unit] * (1f - alignment)
}
