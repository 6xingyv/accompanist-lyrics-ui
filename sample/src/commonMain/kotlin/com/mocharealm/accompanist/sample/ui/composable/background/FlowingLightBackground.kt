package com.mocharealm.accompanist.sample.ui.composable.background

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.sample.ui.utils.composable.CompatBlurImage

@Stable
data class BackgroundVisualState(
    val bitmap: ImageBitmap?,
    val isBright: Boolean
)

@Composable
fun FlowingLightBackground(
    state: BackgroundVisualState,
    modifier: Modifier = Modifier
) {
    if (state.bitmap != null) {
        val colorFilter = if (state.isBright) {
            ColorFilter.tint(Color.Black.copy(alpha = 0.1f), BlendMode.Darken)
        } else {
            null
        }
        Box(
            modifier = modifier
                .graphicsLayer { clip = true }) {
            val baseModifier = Modifier.scale(3f)

            CompatBlurImage(
                bitmap = state.bitmap,
                contentDescription = null,
                colorFilter = colorFilter,
                modifier = baseModifier
                    .align(Alignment.TopStart)
//                    .graphicsLayer { rotationZ = rotation1 }
                ,
                blurRadius = 30.dp
            )

            CompatBlurImage(
                bitmap = state.bitmap,
                contentDescription = null,
                colorFilter = colorFilter,
                modifier = baseModifier
                    .align(Alignment.Center)
//                    .graphicsLayer { rotationZ = rotation2 }
                ,
                blurRadius = 30.dp
            )

            CompatBlurImage(
                bitmap = state.bitmap,
                contentDescription = null,
                colorFilter = colorFilter,
                modifier = baseModifier
                    .align(Alignment.BottomEnd)
                    .scale(2f)
                    .graphicsLayer {
//                        rotationZ = rotation3
                        translationX = 50f
                    }
                ,
                blurRadius = 50.dp
            )
        }
    }
}