package com.mocharealm.accompanist.sample.ui.utils.composable

import androidx.compose.runtime.Composable
import kotlinx.coroutines.flow.Flow

@Composable
actual fun CompatBackHandler(
    enabled: Boolean,
    onBack: suspend (@JvmSuppressWildcards Flow<UniBackEvent>) -> @JvmSuppressWildcards Unit
) {

}