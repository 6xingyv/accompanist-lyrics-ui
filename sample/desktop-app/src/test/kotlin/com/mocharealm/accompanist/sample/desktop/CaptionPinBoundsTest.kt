package com.mocharealm.accompanist.sample.desktop

import kotlin.test.*

class CaptionPinBoundsTest {
    @Test
    fun windowsPinSharesTheNativeButtonRowAndTracksItsInsets() {
        for (width in listOf(320f, 600f, 1600f)) {
            for (inset in listOf(138f, 180f)) {
                val bounds = captionPinBounds(DesktopPlatform.Windows, width, 0f, inset)
                assertEquals(width - inset, bounds.right, "Pin touches the native controls with no gap")
                assertEquals(inset / 3, bounds.width, "Pin matches a native caption button's width")
                assertEquals(0f, bounds.top)
                assertEquals(CAPTION_HEIGHT, bounds.height)
                assertTrue(bounds.contains(bounds.left + bounds.width / 2, 18f))
                assertFalse(bounds.contains(width - inset, 18f), "Native minimize must stay a native hit")
                assertFalse(bounds.contains(bounds.left - 1, 18f), "Title remains draggable")
                assertFalse(bounds.contains(bounds.left + 1, 36f), "Player content remains outside the caption")
            }
        }
    }

    @Test
    fun macPinSitsBesideTheLeftTrafficLights() {
        val narrow = captionPinBounds(DesktopPlatform.Mac, 320f, 80f, 0f)
        val wide = captionPinBounds(DesktopPlatform.Mac, 1600f, 80f, 0f)
        assertEquals(narrow, wide, "Mac control placement follows the left inset, not window width")
        assertTrue(narrow.left > 80f)
        assertEquals(CAPTION_HEIGHT / 2, narrow.top + narrow.height / 2)
        assertFalse(narrow.contains(79f, 18f))
    }
}
