package com.mocharealm.accompanist.sample.desktop

import kotlin.test.*

class DesktopLyricsScaleTest {
    @Test
    fun growsContinuouslyAndStopsAtTheReferenceMaximum() {
        assertEquals(1f, desktopLyricsScale(1024f, 512f))
        assertEquals(1.2f, desktopLyricsScale(1312f, 756f), 0.0001f)
        assertEquals(1.4f, desktopLyricsScale(1600f, 1000f), 0.0001f)
        assertEquals(1.4f, desktopLyricsScale(2560f, 1440f), 0.0001f)
        val below = desktopLyricsScale(1311f, 756f)
        val above = desktopLyricsScale(1313f, 756f)
        assertTrue(above >= below && above - below < .001f, "Resizing must not jump between font sizes")
    }

    @Test
    fun aShortOrNarrowWindowLimitsFontGrowth() {
        assertEquals(1f, desktopLyricsScale(2560f, 400f))
        assertEquals(1.2f, desktopLyricsScale(2560f, 756f), 0.0001f)
        assertEquals(1f, desktopLyricsScale(1000f, 700f))
    }

    @Test
    fun compactAndPortraitWindowsKeepTheirBaseFontSize() {
        assertEquals(1f, desktopLyricsScale(420f, 520f))
        assertEquals(1f, desktopLyricsScale(1600f, 1600f))
    }
}
