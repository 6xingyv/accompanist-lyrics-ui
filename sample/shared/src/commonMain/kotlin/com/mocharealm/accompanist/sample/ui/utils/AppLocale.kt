package com.mocharealm.accompanist.sample.ui.utils

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ProvidedValue
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.mocharealm.accompanist.sample.ui.adaptive.AdaptiveLayoutProvider

var customAppLocale by mutableStateOf<String?>(null)
expect object LocalAppLocale {
    val current: String @Composable get
    @Composable infix fun provides(value: String?): ProvidedValue<*>
}

@Composable
fun AppEnvironment(content: @Composable () -> Unit) {
    AdaptiveLayoutProvider {
        CompositionLocalProvider(
            LocalAppLocale provides customAppLocale,
        ) {
            key(customAppLocale) {
                content()
            }
        }
    }
}