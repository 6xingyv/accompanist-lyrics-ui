package com.mocharealm.accompanist.sample.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import com.mocharealm.accompanist.sample.ui.utils.AppEnvironment

@Composable
actual fun AccompanistTheme(
    darkTheme: Boolean,
    dynamicColor: Boolean,
    content: @Composable (() -> Unit)
) {
    val colorScheme =
        if (darkTheme) DarkColorScheme
        else LightColorScheme


    MaterialTheme(
        colorScheme = colorScheme,
        typography = Typography(),
    ) {
        AppEnvironment {
            content()
        }
    }
}