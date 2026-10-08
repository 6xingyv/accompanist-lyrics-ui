package com.mocharealm.accompanist.sample.ui.screen.player

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.SystemClock
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.captionBarPadding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.ui.composable.list.LyricsLazyListState
import com.mocharealm.accompanist.lyrics.ui.composable.list.rememberLyricsLazyListState
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.LyricsFade
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.LyricsAnchor
import com.mocharealm.accompanist.sample.Res
import com.mocharealm.accompanist.sample.data.repository.MusicRepositoryImpl
import com.mocharealm.accompanist.sample.domain.model.MusicItem
import com.mocharealm.accompanist.sample.empty
import com.mocharealm.accompanist.sample.ui.adaptive.LocalWindowLayoutType
import com.mocharealm.accompanist.sample.ui.adaptive.WindowLayoutType
import com.mocharealm.accompanist.sample.ui.composable.ModalScaffold
import com.mocharealm.accompanist.sample.ui.composable.background.BackgroundVisualState
import com.mocharealm.accompanist.sample.ui.composable.player.PlayerLayout
import com.mocharealm.accompanist.sample.ui.composable.player.PlayerSurface
import com.mocharealm.accompanist.sample.ui.playback.rememberPlaybackPosition
import com.mocharealm.accompanist.sample.ui.screen.share.ShareContext
import com.mocharealm.accompanist.sample.ui.screen.share.ShareScreen
import com.mocharealm.accompanist.sample.ui.screen.share.ShareViewModel
import com.mocharealm.accompanist.sample.ui.composable.player.PlayerControls
import com.mocharealm.accompanist.sample.ui.composable.player.SongSelectionDialogContent
import com.mocharealm.accompanist.sample.ui.composable.player.PlayerLyricsPanel
import org.jetbrains.compose.resources.imageResource
import org.koin.compose.viewmodel.koinViewModel

@Composable
fun PlayerScreen(
    playerViewModel: PlayerViewModel = koinViewModel(),
    shareViewModel: ShareViewModel = koinViewModel(),
) {
    val listState = rememberLyricsLazyListState()
    val animatedPositionState =
        rememberPlaybackPosition(playerViewModel.playbackState, SystemClock::uptimeMillis)
    val currentPositionProvider =
        remember(animatedPositionState, playerViewModel) {
            {
                val position = animatedPositionState.intValue
                playerViewModel.recordPlaybackTiming(position)
                position
            }
        }
    val uiStateState = playerViewModel.uiState.collectAsState()

    Box(modifier = Modifier.fillMaxSize()) {
        val isShareSheetVisible by
        remember(uiStateState) { derivedStateOf { uiStateState.value.isShareSheetVisible } }
        ModalScaffold(
            isModalOpen = isShareSheetVisible,
            modifier = Modifier.fillMaxSize(),
            onDismissRequest = {
                playerViewModel.onShareDismissed()
                shareViewModel.reset()
            },
            modalContent = { ShareScreen(it, shareViewModel = shareViewModel) },
        ) {
            val backgroundState by
            remember(uiStateState) { derivedStateOf { uiStateState.value.backgroundState } }
            PlayerSurface(backgroundState) {
                val artworkData by
                remember(uiStateState) { derivedStateOf { uiStateState.value.artworkData } }

                val currentMusicItem by
                remember(uiStateState) { derivedStateOf { uiStateState.value.currentMusicItem } }
                val showTranslation by
                remember(uiStateState) { derivedStateOf { uiStateState.value.showTranslation } }
                val showPhonetic by
                remember(uiStateState) { derivedStateOf { uiStateState.value.showPhonetic } }
                val lyrics by
                remember(uiStateState) { derivedStateOf { uiStateState.value.lyrics } }

                AndroidPlayerContent(
                    listState = listState,
                    animatedPosition = currentPositionProvider,
                    playerViewModel = playerViewModel,
                    shareViewModel = shareViewModel,
                    backgroundState = backgroundState,
                    artworkData = artworkData,
                    currentMusicItem = currentMusicItem,
                    showTranslation = showTranslation,
                    showPhonetic = showPhonetic,
                    lyrics = lyrics,
                )

                val showSelectionDialog by
                remember(uiStateState) { derivedStateOf { uiStateState.value.showSelectionDialog } }
                if (showSelectionDialog) {
                    val importing by
                    remember(uiStateState) { derivedStateOf { uiStateState.value.isImporting } }
                    val ready by
                    remember(uiStateState) { derivedStateOf { uiStateState.value.isReady } }
                    val selectionError by
                    remember(uiStateState) { derivedStateOf { uiStateState.value.selectionError } }
                    SongSelectionDialog(
                        onSongSelected = { audio, lyrics, translation ->
                            playerViewModel.onFilesSelected(audio, lyrics, translation)
                        },
                        onDismissRequest = playerViewModel::onDismissSongSelection,
                        isImporting = importing,
                        isReady = ready,
                        error = selectionError,
                    )
                }
            }
        }
    }
}

@Composable
private fun AndroidPlayerContent(
    listState: LyricsLazyListState,
    animatedPosition: () -> Int,
    playerViewModel: PlayerViewModel,
    shareViewModel: ShareViewModel,
    backgroundState: BackgroundVisualState,
    artworkData: ByteArray?,
    currentMusicItem: MusicItem?,
    showTranslation: Boolean,
    showPhonetic: Boolean,
    lyrics: SyncedLyrics?,
) {
    PlayerLayout(
        title = currentMusicItem?.label ?: "Unknown Title",
        artist = currentMusicItem?.artist ?: "Unknown",
        metadataTimeMillis = animatedPosition,
        hasArtwork = artworkData != null,
        artwork = { modifier ->
            PlayerArtwork(artworkData, imageResource(Res.drawable.empty), modifier)
        },
        modifier = Modifier.captionBarPadding().statusBarsPadding(),
        compact = LocalWindowLayoutType.current == WindowLayoutType.Phone,
        showLyricsInWideLayout = lyrics != null,
        controls = {
            PlayerControls(
                onOpenSongSelection = playerViewModel::onOpenSongSelection,
                showTranslation = showTranslation,
                showPhonetic = showPhonetic,
                onToggleTranslation = playerViewModel::toggleTranslation,
                onTogglePhonetic = playerViewModel::togglePhonetic,
            )
        },
        lyrics = { modifier, anchor, bottomFade ->
            val cover = (backgroundState.bitmap ?: imageResource(Res.drawable.empty)).asAndroidBitmap()
            PlayerLyrics(
                listState = listState,
                lyrics = lyrics,
                currentPosition = animatedPosition,
                showTranslation = showTranslation,
                showPhonetic = showPhonetic,
                onSeekTo = playerViewModel::seekTo,
                onShare = { line ->
                    lyrics?.let {
                        playerViewModel.onShareRequested()
                        val context = ShareContext(
                            lyrics = it,
                            initialLine = line,
                            backgroundState = backgroundState,
                            title = currentMusicItem?.label ?: "Unknown Title",
                            artist = currentMusicItem?.artist ?: "Unknown",
                            cover = cover,
                            artworkData = artworkData,
                        )
                        shareViewModel.prepareForSharing(context)
                        playerViewModel.onShareRequested()
                    }
                },
                modifier = modifier,
                anchor = anchor,
                bottomFade = bottomFade,
            )
        },
    )
}

@Composable
fun SongSelectionDialog(
    onDismissRequest: () -> Unit,
    onSongSelected: (Uri, Uri?, Uri?) -> Unit,
    isImporting: Boolean,
    isReady: Boolean,
    error: String?,
) {
    val context = LocalContext.current
    var audioUri by remember { mutableStateOf<Uri?>(null) }
    var lyricsUri by remember { mutableStateOf<Uri?>(null) }
    var translationUri by remember { mutableStateOf<Uri?>(null) }
    var audioName by remember { mutableStateOf("Select audio") }
    var lyricsName by remember { mutableStateOf("Select lyrics (optional)") }
    var translationName by remember { mutableStateOf("Select translation (optional)") }
    val repository = remember(context) { MusicRepositoryImpl(context.applicationContext) }
    fun storageGranted() =
        if (Build.VERSION.SDK_INT >= 30) Environment.isExternalStorageManager()
        else
            context.checkSelfPermission(Manifest.permission.READ_EXTERNAL_STORAGE) ==
                    PackageManager.PERMISSION_GRANTED

    var storageAccess by remember { mutableStateOf(storageGranted()) }
    LaunchedEffect(Unit) {
        val saved = repository.lastSelection()
        if (saved != null && audioUri == null) {
            audioUri = saved.audio
            lyricsUri = saved.lyrics
            translationUri = saved.translation
            audioName = displayNameForUri(context, saved.audio)
            saved.lyrics?.let { lyricsName = displayNameForUri(context, it) }
            saved.translation?.let { translationName = displayNameForUri(context, it) }
        }
    }
    LaunchedEffect(audioUri, storageAccess) {
        val audio = audioUri ?: return@LaunchedEffect
        if (lyricsUri == null) {
            val matched = repository.findExternalLyricsUri(audio)
            if (matched != null && lyricsUri == null) {
                lyricsUri = matched
                lyricsName = displayNameForUri(context, matched)
            }
        }
    }

    fun retain(uri: Uri) {
        runCatching {
            context.contentResolver.takePersistableUriPermission(
                uri,
                android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION,
            )
        }
    }

    val audioLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            if (uri != null) {
                retain(uri)
                audioUri = uri
                audioName = displayNameForUri(context, uri)
                lyricsUri = null
                translationUri = null
                lyricsName = "Select lyrics (optional)"
                translationName = "Select translation (optional)"
            }
        }
    val allFilesLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
            storageAccess = storageGranted()
            if (storageAccess) audioLauncher.launch(arrayOf("audio/*"))
        }
    val legacyStorageLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            storageAccess = granted
            if (granted) audioLauncher.launch(arrayOf("audio/*"))
        }

    fun selectAudio() {
        if (storageGranted()) {
            audioLauncher.launch(arrayOf("audio/*"))
            return
        }
        if (Build.VERSION.SDK_INT >= 30) {
            val intent =
                Intent(
                    Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                    Uri.parse("package:${context.packageName}"),
                )
            try {
                allFilesLauncher.launch(intent)
            } catch (_: android.content.ActivityNotFoundException) {
                allFilesLauncher.launch(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION))
            }
        } else legacyStorageLauncher.launch(Manifest.permission.READ_EXTERNAL_STORAGE)
    }

    val lyricsLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            if (uri != null) {
                retain(uri)
                lyricsUri = uri
                lyricsName = displayNameForUri(context, uri)
            }
        }
    val translationLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            if (uri != null) {
                retain(uri)
                translationUri = uri
                translationName = displayNameForUri(context, uri)
            }
        }
    SongSelectionDialogContent(
        audioName = audioName, lyricsName = lyricsName, translationName = translationName,
        audioSelected = audioUri != null, isImporting = isImporting, isReady = isReady,
        error = error,
        onSelectAudio = ::selectAudio,
        onSelectLyrics = { lyricsLauncher.launch(arrayOf("*/*")) },
        onSelectTranslation = { translationLauncher.launch(arrayOf("*/*")) },
        onPlay = { audioUri?.let { onSongSelected(it, lyricsUri, translationUri) } },
        onDismissRequest = onDismissRequest,
        fileAccessMessage = if (!storageAccess)
            "Allow file access to automatically match lyrics beside the audio file." else null,
    )
}

private fun displayNameForUri(context: android.content.Context, uri: Uri): String =
    runCatching {
        context.contentResolver
            .query(
                uri,
                arrayOf(android.provider.OpenableColumns.DISPLAY_NAME),
                null,
                null,
                null,
            )
            ?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
    }
        .getOrNull() ?: uri.path?.substringAfterLast('/') ?: "Selected file"

@Composable
fun PlayerLyrics(
    listState: LyricsLazyListState,
    lyrics: SyncedLyrics?,
    currentPosition: () -> Int,
    showTranslation: Boolean,
    showPhonetic: Boolean,
    onSeekTo: (Int) -> Unit,
    onShare: (KaraokeLine) -> Unit,
    modifier: Modifier = Modifier,
    anchor: LyricsAnchor = LyricsAnchor.Fixed(64.dp),
    bottomFade: LyricsFade = LyricsFade.Fraction(0.5f),
) {
    PlayerLyricsPanel(
        anchor = anchor,
        bottomFade = bottomFade,
        listState = listState,
        lyrics = lyrics,
        currentPosition = currentPosition,
        onLineClicked = { line -> onSeekTo(line.start) },
        onLinePressed = { line ->
            when (line) {
                is KaraokeLine -> onShare(line)
                is SyncedLine ->
                    onShare(
                        KaraokeLine.MainKaraokeLine(
                            listOf(
                                com.mocharealm.accompanist.lyrics.core.model.karaoke
                                    .KaraokeSyllable(line.content, line.start, line.end)
                            ),
                            line.translation,
                            com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
                                .Start,
                            line.start,
                            line.end,
                        )
                    )
            }
        },
        showTranslation = showTranslation,
        showPhonetic = showPhonetic,
        modifier = modifier,
    )
}
