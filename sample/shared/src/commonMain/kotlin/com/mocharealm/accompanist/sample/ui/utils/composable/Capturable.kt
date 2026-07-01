package com.mocharealm.accompanist.sample.ui.utils.composable

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.layer.drawLayer
import androidx.compose.ui.graphics.rememberGraphicsLayer
import kotlinx.coroutines.launch

class CapturableController {
    internal var triggerCapture: ((onCaptured: (ImageBitmap) -> Unit) -> Unit)? = null

    /**
     * 发出一个截图请求，并提供一个截图成功后的回调。
     * @param onCaptured 截图成功后将被调用的函数，它会接收到 ImageBitmap。
     */
    fun capture(onCaptured: (ImageBitmap) -> Unit) {
        // 调用内部触发器，并将 onCaptured 回调传递进去
        triggerCapture?.invoke(onCaptured)
    }
}

@Composable
fun rememberCapturableController(): CapturableController {
    return remember { CapturableController() }
}

@Composable
fun Capturable(
    controller: CapturableController,
    modifier: Modifier = Modifier,
    content: @Composable BoxScope.() -> Unit
) {
    val coroutineScope = rememberCoroutineScope()
    val graphicsLayer = rememberGraphicsLayer()

    controller.triggerCapture = { onCapturedCallback ->
        coroutineScope.launch {
            val imageBitmap = graphicsLayer.toImageBitmap()
            // 当截图完成后，执行从 capture 函数传进来的回调
            onCapturedCallback(imageBitmap)
        }
    }

    Box(
        modifier = modifier.drawWithContent {
            graphicsLayer.record {
                this@drawWithContent.drawContent()
            }
            drawLayer(graphicsLayer)
        }
    ) {
        content()
    }
}