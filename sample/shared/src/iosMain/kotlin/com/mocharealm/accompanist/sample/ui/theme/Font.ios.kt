package com.mocharealm.accompanist.sample.ui.theme

import androidx.compose.runtime.Composable
import androidx.compose.ui.text.font.FontFamily

// Resolve the system face on iOS, including native weight and language fallback.
@Composable
actual fun SFPro(): FontFamily = FontFamily.Default
