package com.mocharealm.accompanist.sample.ui.composable.background

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ColorMatrix
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.sample.Res
import com.mocharealm.accompanist.sample.empty
import com.mocharealm.accompanist.sample.ui.utils.composable.CompatBlurImage
import org.jetbrains.compose.resources.imageResource

@Stable
data class BackgroundVisualState(
    val bitmap: ImageBitmap?,
    val luminance: Float
)

@Composable
fun BoxScope.FlowingLightBackground(
    state: BackgroundVisualState,
    modifier: Modifier = Modifier
) {
//    val infiniteTransition = rememberInfiniteTransition(label = "background_animation")
//
//    val rotation1 by infiniteTransition.animateFloat(
//        initialValue = 0f,
//        targetValue = 360f,
//        animationSpec = infiniteRepeatable(
//            animation = tween(50000, easing = LinearEasing),
//            repeatMode = RepeatMode.Restart
//        ),
//        label = "rotation1"
//    )
//    val rotation2 by infiniteTransition.animateFloat(
//        initialValue = 360f,
//        targetValue = 0f,
//        animationSpec = infiniteRepeatable(
//            animation = tween(40000, easing = LinearEasing),
//            repeatMode = RepeatMode.Restart
//        ),
//        label = "rotation2"
//    )
//    val rotation3 by infiniteTransition.animateFloat(
//        initialValue = 0f,
//        targetValue = 360f,
//        animationSpec = infiniteRepeatable(
//            animation = tween(48000, easing = LinearEasing),
//            repeatMode = RepeatMode.Restart
//        ),
//        label = "rotation3"
//    )

    AnimatedContent(
        state.bitmap ?: imageResource(Res.drawable.empty),
        modifier = modifier.matchParentSize(),
        transitionSpec = { fadeIn(tween(600)) togetherWith fadeOut(tween(600)) }
    ) { bitmap ->
        val colorFilter = if (state.luminance > 0.5f) {
            val extra = (state.luminance - 0.5f).coerceIn(0f, 0.5f)
            val darkScale = 1f - extra
            val saturation = 1f + extra * 4f
            val cm = ColorMatrix().apply {
                setToSaturation(saturation)
                setToScale(darkScale, darkScale, darkScale, 1f)
            }
            ColorFilter.colorMatrix(cm)
        } else {
            null
        }
        Box(
            modifier = Modifier
                .graphicsLayer { clip = true }) {
            val baseModifier = Modifier.size(800.dp)

            CompatBlurImage(
                bitmap = bitmap,
                contentDescription = null,
                colorFilter = colorFilter,
                modifier = Modifier
                    .matchParentSize()
                    .scale(2.5f),
                blurRadius = 25.dp
            )

            CompatBlurImage(
                bitmap = bitmap,
                contentDescription = null,
                colorFilter = colorFilter,
                modifier = baseModifier
                    .align(Alignment.TopStart)
                    .offset((-150).dp, (-150).dp)
                    .scale(2.5f)
//                    .graphicsLayer { rotationZ = rotation1 }
                ,
                blurRadius = 80.dp
            )

            CompatBlurImage(
                bitmap = bitmap,
                contentDescription = null,
                colorFilter = colorFilter,
                modifier = baseModifier
                    .align(Alignment.Center)
                    .scale(1.5f)
//                    .graphicsLayer {
//                        rotationZ = rotation2
//                        transformOrigin = TransformOrigin(0.3f, 0.52f)
//                    }
                ,
                blurRadius = 100.dp
            )

            CompatBlurImage(
                bitmap = bitmap,
                contentDescription = null,
                colorFilter = colorFilter,
                modifier = baseModifier
                    .align(Alignment.BottomEnd)
                    .offset((150).dp, (150).dp)
                    .scale(2f)
//                    .graphicsLayer {
//                        rotationZ = rotation3
//                        translationX = 50f
//                    }
                ,
                blurRadius = 150.dp
            )
        }
    }
}