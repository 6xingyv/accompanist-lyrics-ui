package com.mocharealm.accompanist.sample.ui.utils.composable

import androidx.annotation.FloatRange
import androidx.compose.runtime.Composable
import kotlinx.coroutines.flow.Flow

@Composable
expect fun CompatBackHandler(
    enabled: Boolean = true,
    onBack: 
    suspend (progress: @JvmSuppressWildcards Flow<UniBackEvent>) -> @JvmSuppressWildcards
    Unit
)

class UniBackEvent(
    val touchX: Float,
    val touchY: Float,
    @FloatRange(from = 0.0, to = 1.0) val progress: Float,
    val swipeEdge: SwipeEdge
) {
    enum class SwipeEdge(val value: Int) {
        LEFT(0),
        RIGHT(1)
    }

    override fun toString(): String {
        return "BackEventCompat{touchX=$touchX, touchY=$touchY, progress=$progress, " +
                "swipeEdge=$swipeEdge}"
    }
}


