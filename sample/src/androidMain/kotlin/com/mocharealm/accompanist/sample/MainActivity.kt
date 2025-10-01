package com.mocharealm.accompanist.sample

import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalConfiguration
import com.mocharealm.accompanist.sample.service.PlaybackService
import com.mocharealm.accompanist.sample.ui.screen.player.PlayerScreen
import com.mocharealm.accompanist.sample.ui.theme.AccompanistTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val intent = Intent(this, PlaybackService::class.java)
        startService(intent)

        enableEdgeToEdge(
            navigationBarStyle = SystemBarStyle.auto(
                Color.Transparent.toArgb(), Color.Transparent.toArgb()
            ),
            statusBarStyle = SystemBarStyle.dark(Color.White.toArgb())
        )
        // FUCKING XIAOMI
        @Suppress("DEPRECATION") if (Build.MANUFACTURER == "Xiaomi") {
            window.addFlags(WindowManager.LayoutParams.FLAG_TRANSLUCENT_NAVIGATION)
        }

        setContent {
            AccompanistTheme {
                PlayerScreen()
            }
        }
    }
}