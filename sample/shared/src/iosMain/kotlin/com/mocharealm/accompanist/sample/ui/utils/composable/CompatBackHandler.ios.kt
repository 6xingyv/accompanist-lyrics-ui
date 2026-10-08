package com.mocharealm.accompanist.sample.ui.utils.composable

import androidx.compose.runtime.Composable
import kotlinx.coroutines.flow.Flow

// iOS navigation gestures are owned by the hosting UIKit/SwiftUI controller.
@Composable
actual fun CompatBackHandler(enabled: Boolean, onBack: suspend (progress: Flow<UniBackEvent>) -> Unit) {}
