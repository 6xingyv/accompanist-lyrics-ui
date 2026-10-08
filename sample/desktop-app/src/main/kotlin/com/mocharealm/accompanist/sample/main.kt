package com.mocharealm.accompanist.sample

import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.*
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.onPointerEvent
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.*
import com.mocharealm.accompanist.lyrics.ui.composable.list.rememberLyricsLazyListState
import com.mocharealm.accompanist.sample.desktop.*
import com.mocharealm.accompanist.sample.ui.composable.player.*
import com.mocharealm.accompanist.sample.ui.theme.AccompanistTheme
import java.awt.Dimension
import java.awt.FileDialog

fun main(args: Array<String>) {
    val config = try { DesktopConfig.load(args) } catch (failure: Exception) {
        System.err.println(failure.message)
        return
    }
    // Skiko reads GPU properties when its first surface is created.
    configureDesktopGraphics(config)
    application {
        val controller = remember { DesktopController(config, configLoader = { DesktopConfig.load(args) }) }
        val currentConfig by controller.configuration.collectAsState()
        val windowState = rememberWindowState(width = config.width.dp, height = config.height.dp)
        var pinned by remember { mutableStateOf(config.alwaysOnTop) }
        DisposableEffect(controller) { onDispose { controller.close() } }
        Window(onCloseRequest = ::exitApplication, state = windowState,
            title = "Accompanist Desktop Lyrics", undecorated = false,
            resizable = true, alwaysOnTop = pinned) {
            LaunchedEffect(window, currentConfig) {
                window.minimumSize = Dimension(currentConfig.minWidth, currentConfig.minHeight)
                windowState.size = androidx.compose.ui.unit.DpSize(currentConfig.width.dp, currentConfig.height.dp)
                pinned = currentConfig.alwaysOnTop
            }
            AccompanistTheme(darkTheme = true) {
                DesktopPlayer(controller, pinned, { pinned = !pinned },
                    ::exitApplication)
            }
        }
    }
}

@OptIn(ExperimentalComposeUiApi::class)
@Composable
private fun FrameWindowScope.DesktopPlayer(
    controller: DesktopController,
    pinned: Boolean,
    togglePinned: () -> Unit,
    close: () -> Unit,
) {
    val ui by controller.state.collectAsState()
    DesktopActionErrors(controller)
    val position = rememberDesktopPosition(controller.playback)
    val listState = rememberLyricsLazyListState()
    var captionHovered by remember { mutableStateOf(false) }
    var decoration by remember { mutableStateOf<JbrWindowDecoration?>(null) }
    DisposableEffect(window) {
        val installed = JbrWindowDecoration.install(window, onHover = { captionHovered = it })
        decoration = installed
        onDispose { installed?.close() }
    }
    var menuOpen by remember { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    val openPreview = {
        val picker = FileDialog(window, "Open timed lyrics", FileDialog.LOAD)
        try {
            picker.isVisible = true
            picker.file?.let { controller.openPreview(java.nio.file.Path.of(picker.directory, it)) }
        } finally { picker.dispose() }
    }
    LaunchedEffect(Unit) { focus.requestFocus() }
    PlayerSurface(ui.background, Modifier.fillMaxSize().onPreviewKeyEvent { event ->
        if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
        val primary = if (desktopPlatform() == DesktopPlatform.Mac) event.isMetaPressed else event.isCtrlPressed
        when {
            primary && event.key == Key.O -> { openPreview(); true }
            primary && event.key == Key.W -> { close(); true }
            primary && event.key == Key.R -> { controller.reloadLyrics(); true }
            primary && event.key == Key.P -> { togglePinned(); true }
            primary && event.key == Key.DirectionLeft && ui.canSeek -> {
                controller.seek(position.intValue.toLong() - 5000); true
            }
            primary && event.key == Key.DirectionRight && ui.canSeek -> {
                controller.seek(position.intValue.toLong() + 5000); true
            }
            event.key == Key.Spacebar && (if (ui.following) ui.source.isNotBlank() else ui.canSeek) -> {
                controller.togglePlayback(); true
            }
            else -> false
        }
    }.focusRequester(focus).focusable()) {
        Column(Modifier.fillMaxSize()) {
            DesktopTitleBar(
                platform = desktopPlatform(),
                leftInset = decoration?.leftInset ?: if (desktopPlatform() == DesktopPlatform.Mac) 80f else 0f,
                rightInset = decoration?.rightInset ?: if (desktopPlatform() == DesktopPlatform.Windows) 138f else 0f,
                hovered = captionHovered,
                pinned = pinned,
                togglePinned = togglePinned,
                modifier = Modifier
                    .onPointerEvent(PointerEventType.Enter) { captionHovered = true }
                    .onPointerEvent(PointerEventType.Exit) { captionHovered = false },
            )
            BoxWithConstraints(Modifier.weight(1f)) {
                val fontScale = desktopLyricsScale(maxWidth.value, maxHeight.value)
                PlayerLayout(
                    title = ui.title.ifBlank { "Unknown Title" },
                    artist = ui.artist,
                    metadataTimeMillis = { position.intValue },
                    hasArtwork = ui.background.bitmap != null,
                    artwork = { modifier ->
                        PlayerCover(ui.background.bitmap, modifier)
                    },
                    controls = {
                        Box {
                            PlayerControls({ menuOpen = true }, ui.showTranslation, ui.showPhonetic,
                                controller::toggleTranslation, controller::togglePhonetic)
                            DesktopMoreMenu(menuOpen, { menuOpen = false },
                                controller::rescanMediaSessions, controller::reloadLyrics,
                                controller::reloadConfiguration, controller::showConfigurationLocation)
                        }
                    },
                    lyrics = { modifier, anchor, bottomFade ->
                        val lyrics = ui.lyrics
                        key(lyrics) {
                            PlayerLyricsPanel(listState = listState, lyrics = lyrics,
                                currentPosition = { position.intValue },
                                showTranslation = ui.showTranslation, showPhonetic = ui.showPhonetic,
                                onLineClicked = { controller.seek(it.start.toLong()) },
                                modifier = modifier, anchor = anchor, bottomFade = bottomFade,
                                loading = ui.loading, fontScale = fontScale,
                                emptyMessage = ui.message ?: if (ui.following)
                                    "Play music to display matching local lyrics."
                                    else "Open a lyrics file to preview.")
                        }
                    },
                )
            }
        }
    }
}
