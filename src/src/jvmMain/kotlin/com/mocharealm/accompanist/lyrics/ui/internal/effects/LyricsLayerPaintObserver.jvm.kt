package com.mocharealm.accompanist.lyrics.ui.internal.effects

import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent

internal actual fun Modifier.lyricsLayerPaintObserver(observePaint: () -> Unit): Modifier =
    drawWithContent {
        observePaint()
        drawContent()
    }
