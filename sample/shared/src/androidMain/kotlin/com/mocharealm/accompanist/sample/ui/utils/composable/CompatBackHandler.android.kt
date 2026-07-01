package com.mocharealm.accompanist.sample.ui.utils.composable

import android.annotation.SuppressLint
import androidx.activity.BackEventCompat
import androidx.activity.OnBackPressedCallback
import androidx.activity.compose.LocalOnBackPressedDispatcherOwner
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.platform.LocalLifecycleOwner
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.BufferOverflow.SUSPEND
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.Channel.Factory.BUFFERED
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.consumeAsFlow
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.launch
import java.util.concurrent.CancellationException

@SuppressLint("RememberReturnType") // TODO: b/372566999
@Composable
actual fun CompatBackHandler(
    enabled: Boolean,
    onBack:
    suspend (progress: @JvmSuppressWildcards Flow<UniBackEvent>) -> @JvmSuppressWildcards
    Unit
) {
    // ensure we don't re-register callbacks when onBack changes
    val currentOnBack by rememberUpdatedState(onBack)
    val onBackScope = rememberCoroutineScope()

    val backCallBack = remember {
        PredictiveBackHandlerCallback(enabled, onBackScope, currentOnBack)
    }

    // we want to use the same callback, but ensure we adjust the variable on recomposition
    remember(currentOnBack, onBackScope) {
        backCallBack.currentOnBack = currentOnBack
        backCallBack.onBackScope = onBackScope
    }

    LaunchedEffect(enabled) { backCallBack.setIsEnabled(enabled) }

    val backDispatcher =
        checkNotNull(LocalOnBackPressedDispatcherOwner.current) {
            "No OnBackPressedDispatcherOwner was provided via LocalOnBackPressedDispatcherOwner"
        }
            .onBackPressedDispatcher

    @Suppress("deprecation", "KotlinRedundantDiagnosticSuppress") // TODO b/330570365
    val lifecycleOwner = LocalLifecycleOwner.current

    DisposableEffect(lifecycleOwner, backDispatcher) {
        backDispatcher.addCallback(lifecycleOwner, backCallBack)

        onDispose { backCallBack.remove() }
    }
}

fun BackEventCompat.toUniBackEvent(): UniBackEvent =
    UniBackEvent(
        touchX = this.touchX,
        touchY = this.touchY,
        progress = this.progress,
        swipeEdge = when (this.swipeEdge) {
            BackEventCompat.EDGE_LEFT -> UniBackEvent.SwipeEdge.LEFT
            BackEventCompat.EDGE_RIGHT -> UniBackEvent.SwipeEdge.RIGHT
            else -> throw IllegalStateException("Unknown swipe edge: ${this.swipeEdge}")
        }
    )

private class OnBackInstance(
    scope: CoroutineScope,
    var isPredictiveBack: Boolean,
    onBack: suspend (progress: Flow<UniBackEvent>) -> Unit,
    callback: OnBackPressedCallback
) {
    val channel = Channel<UniBackEvent>(capacity = BUFFERED, onBufferOverflow = SUSPEND)
    val job =
        scope.launch {
            if (callback.isEnabled) {
                var completed = false
                onBack(channel.consumeAsFlow().onCompletion { completed = true })
                check(completed) { "You must collect the progress flow" }
            }
        }

    fun send(backEvent: UniBackEvent) = channel.trySend(backEvent)

    // idempotent if invoked more than once
    fun close() = channel.close()

    fun cancel() {
        channel.cancel(CancellationException("onBack cancelled"))
        job.cancel()
    }
}

private class PredictiveBackHandlerCallback(
    enabled: Boolean,
    var onBackScope: CoroutineScope,
    var currentOnBack: suspend (progress: Flow<UniBackEvent>) -> Unit,
) : OnBackPressedCallback(enabled) {
    private var onBackInstance: OnBackInstance? = null
    private var isActive = false

    fun setIsEnabled(enabled: Boolean) {
        // We are disabling a callback that was enabled.
        if (!enabled && !isActive && isEnabled) {
            onBackInstance?.cancel()
        }
        isEnabled = enabled
    }

    override fun handleOnBackStarted(backEvent: BackEventCompat) {
        super.handleOnBackStarted(backEvent)
        // in case the previous onBackInstance was started by a normal back gesture
        // we want to make sure it's still cancelled before we start a predictive
        // back gesture
        onBackInstance?.cancel()
        if (isEnabled) {
            onBackInstance = OnBackInstance(onBackScope, true, currentOnBack, this)
        }
        isActive = true
    }

    override fun handleOnBackProgressed(backEvent: BackEventCompat) {
        super.handleOnBackProgressed(backEvent)
        onBackInstance?.send(backEvent.toUniBackEvent())
    }

    override fun handleOnBackPressed() {
        // handleOnBackPressed could be called by regular back to restart
        // a new back instance. If this is the case (where current back instance
        // was NOT started by handleOnBackStarted) then we need to reset the previous
        // regular back.
        onBackInstance?.apply {
            if (!isPredictiveBack) {
                cancel()
                onBackInstance = null
            }
        }
        if (onBackInstance == null) {
            onBackInstance = OnBackInstance(onBackScope, false, currentOnBack, this)
        }

        // finally, we close the channel to ensure no more events can be sent
        // but let the job complete normally
        onBackInstance?.close()
        onBackInstance?.isPredictiveBack = false
        isActive = false
    }

    override fun handleOnBackCancelled() {
        super.handleOnBackCancelled()
        // cancel will purge the channel of any sent events that are yet to be received
        onBackInstance?.cancel()
        onBackInstance?.isPredictiveBack = false
        isActive = false
    }
}
