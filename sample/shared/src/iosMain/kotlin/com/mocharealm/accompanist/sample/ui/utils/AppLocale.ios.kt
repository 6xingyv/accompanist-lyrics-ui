package com.mocharealm.accompanist.sample.ui.utils

import androidx.compose.runtime.Composable
import androidx.compose.runtime.ProvidedValue
import androidx.compose.runtime.staticCompositionLocalOf
import platform.Foundation.*

actual object LocalAppLocale {
    private val locale = staticCompositionLocalOf { NSLocale.currentLocale.localeIdentifier }
    actual val current: String
        @Composable get() = locale.current
    @Composable
    actual infix fun provides(value: String?): ProvidedValue<*> =
        locale.provides(value ?: NSLocale.currentLocale.localeIdentifier)
}
