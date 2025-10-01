package com.mocharealm.accompanist.sample.ui.utils.composable

import androidx.activity.BackEventCompat
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.runtime.Composable
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map

@Composable
actual fun CompatBackHandler(
    enabled: Boolean,
    onBack:
    suspend (progress: @JvmSuppressWildcards Flow<UniBackEvent>) -> @JvmSuppressWildcards
    Unit
) =
    PredictiveBackHandler(enabled = enabled) { backEvents ->
        backEvents
            .toUniBackEvent()
            .collect { event ->
                onBack(flowOf(event))
            }
    }

fun Flow<BackEventCompat>.toUniBackEvent(): Flow<UniBackEvent> = this.map {
    UniBackEvent(
        touchX = it.touchX,
        touchY = it.touchY,
        progress = it.progress,
        swipeEdge = when (it.swipeEdge) {
            BackEventCompat.EDGE_LEFT -> UniBackEvent.SwipeEdge.LEFT
            BackEventCompat.EDGE_RIGHT -> UniBackEvent.SwipeEdge.RIGHT
            else -> throw IllegalStateException("Unknown swipe edge: ${it.swipeEdge}")
        }
    )
}