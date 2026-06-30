package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.runtime.Composable
import androidx.compose.ui.text.font.FontFamily
import com.mocharealm.accompanist.lyrics.text.NativeFontSource

@Composable
actual fun getPlatformContext(): Any? {
    return null  // Apple platforms don't need context
}
actual fun getFontSource(fontFamily: FontFamily?, platformContext: Any?): NativeFontSource? {
    return null
}

actual fun getSystemFallbackFontSources(platformContext: Any?): List<NativeFontSource> {
    return emptyList()
}
