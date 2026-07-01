package com.mocharealm.accompanist.sample.ui.theme

import android.os.Build
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import com.mocharealm.accompanist.sample.ui.utils.AppEnvironment

@Composable
actual fun AccompanistTheme(
    darkTheme: Boolean,
    dynamicColor: Boolean,
    content: @Composable (() -> Unit)
) {
    val colorScheme =
        if (dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val context = LocalContext.current
            if (darkTheme) dynamicDarkColorScheme(context)
            else dynamicLightColorScheme(context)
        } else {
            if (darkTheme) DarkColorScheme
            else LightColorScheme
        }

    MaterialTheme(
        colorScheme = colorScheme,
        typography = Typography(),
    ) {
        AppEnvironment {
            content()
        }
    }
}