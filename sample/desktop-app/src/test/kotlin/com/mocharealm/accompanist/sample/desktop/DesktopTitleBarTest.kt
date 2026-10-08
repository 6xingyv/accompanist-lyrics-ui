package com.mocharealm.accompanist.sample.desktop

import androidx.compose.foundation.background
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.InternalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.*
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.platform.FrameRecomposer
import androidx.compose.ui.scene.CanvasLayersComposeScene
import androidx.compose.ui.unit.IntSize
import java.io.File
import kotlin.test.*
import kotlinx.coroutines.Dispatchers

@OptIn(InternalComposeUiApi::class)
class DesktopTitleBarTest {
    @Test fun windowsPinTogglesAtTheNativeCaptionBoundary() = verifyPin(DesktopPlatform.Windows, 0f, 138f)
    @Test fun macPinTogglesBesideTrafficLights() = verifyPin(DesktopPlatform.Mac, 80f, 0f)

    private fun verifyPin(platform: DesktopPlatform, leftInset: Float, rightInset: Float) {
        val recomposer = FrameRecomposer(Dispatchers.Unconfined)
        val scene = CanvasLayersComposeScene(frameRecomposer = recomposer, size = IntSize(600, 36))
        var pinned by mutableStateOf(false)
        var nanos = 0L
        fun settle() {
            repeat(60) {
                Snapshot.sendApplyNotifications()
                nanos += 16_666_667
                recomposer.performFrame(nanos)
                scene.measureAndLayout()
            }
        }
        fun render(name: String): IntArray {
            val bitmap = ImageBitmap(600, 36)
            scene.draw(Canvas(bitmap))
            val destination = File("build/caption-preview/${platform.name.lowercase()}-$name.png")
            destination.parentFile.mkdirs()
            org.jetbrains.skia.Image.makeFromBitmap(bitmap.asSkiaBitmap()).use { image ->
                image.encodeToData()!!.use { destination.writeBytes(it.bytes) }
            }
            return IntArray(600 * 36).also { bitmap.readPixels(it) }
        }
        try {
            scene.setContent {
                DesktopTitleBar(platform, leftInset, rightInset, hovered = true,
                    pinned = pinned, togglePinned = { pinned = !pinned },
                    modifier = Modifier.background(Color(0xFF303030)))
            }
            settle()
            val before = render("unpinned")
            val bounds = captionPinBounds(platform, 600f, leftInset, rightInset)
            val position = Offset(bounds.left + bounds.width / 2, bounds.top + bounds.height / 2)
            scene.sendPointerEvent(PointerEventType.Press, position,
                timeMillis = nanos / 1_000_000, type = PointerType.Touch)
            settle()
            scene.sendPointerEvent(PointerEventType.Release, position,
                timeMillis = nanos / 1_000_000, type = PointerType.Touch)
            settle()
            assertTrue(pinned, "The visible pin and the JBR client hit region must agree")
            val after = render("pinned")
            assertFalse(before.contentEquals(after), "Pin state must change the SF Symbol")
        } finally {
            scene.close()
            recomposer.close()
        }
    }
}
