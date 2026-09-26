package com.mocharealm.accompanist.sample.ui.composable.background

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntSize
import com.mocharealm.accompanist.sample.Res
import com.mocharealm.accompanist.sample.empty
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.jetbrains.compose.resources.imageResource

@Stable data class BackgroundVisualState(val bitmap: ImageBitmap?, val luminance: Float)

@Composable
fun BoxScope.FlowingLightBackground(state: BackgroundVisualState, modifier: Modifier = Modifier) {
    val source = state.bitmap ?: imageResource(Res.drawable.empty)
    val density = LocalDensity.current.density
    var viewport by remember { mutableStateOf(IntSize.Zero) }
    var current by remember { mutableStateOf<ImageBitmap?>(null) }
    var previous by remember { mutableStateOf<ImageBitmap?>(null) }
    val fade = remember { Animatable(1f) }
    LaunchedEffect(source, state.luminance, viewport, density) {
        if (viewport.width <= 0 || viewport.height <= 0) return@LaunchedEffect
        val prepared =
            withContext(Dispatchers.Default) {
                prepareBackground(source, state.luminance, viewport, density)
            }
        previous = current
        current = prepared
        if (previous != null) {
            fade.snapTo(0f)
            fade.animateTo(1f, tween(600))
        }
        previous = null
    }
    Canvas(modifier.matchParentSize().onSizeChanged { viewport = it }) {
        drawRect(Color.Black)
        val destination = IntSize(size.width.toInt(), size.height.toInt())
        previous?.let { drawImage(it, dstSize = destination, filterQuality = FilterQuality.Medium) }
        current?.let {
            drawImage(
                it,
                dstSize = destination,
                alpha = if (previous == null) 1f else fade.value,
                filterQuality = FilterQuality.Medium,
            )
        }
    }
}
