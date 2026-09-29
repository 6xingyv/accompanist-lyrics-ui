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
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.MarqueeSpacing
import androidx.compose.foundation.background
import androidx.compose.foundation.basicMarquee
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.captionBarPadding
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextMotion
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.ui.composable.list.LyricsLazyListState
import com.mocharealm.accompanist.lyrics.ui.composable.list.rememberLyricsLazyListState
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.LyricsFade
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.LyricsAnchor
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.KaraokeLyricsView
import com.mocharealm.accompanist.sample.Res
import com.mocharealm.accompanist.sample.data.repository.MusicRepositoryImpl
import com.mocharealm.accompanist.sample.domain.model.MusicItem
import com.mocharealm.accompanist.sample.empty
import com.mocharealm.accompanist.sample.ic_ellipsis
import com.mocharealm.accompanist.sample.ic_phonetic
import com.mocharealm.accompanist.sample.ic_translation
import com.mocharealm.accompanist.sample.ui.adaptive.LocalWindowLayoutType
import com.mocharealm.accompanist.sample.ui.adaptive.WindowLayoutType
import com.mocharealm.accompanist.sample.ui.composable.ModalScaffold
import com.mocharealm.accompanist.sample.ui.composable.background.BackgroundVisualState
import com.mocharealm.accompanist.sample.ui.composable.background.FlowingLightBackground
import com.mocharealm.accompanist.sample.ui.playback.rememberPlaybackPosition
import com.mocharealm.accompanist.sample.ui.screen.share.ShareContext
import com.mocharealm.accompanist.sample.ui.screen.share.ShareScreen
import com.mocharealm.accompanist.sample.ui.screen.share.ShareViewModel
import com.mocharealm.accompanist.sample.ui.theme.SFPro
import com.mocharealm.gaze.capsule.ContinuousRoundedRectangle
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.imageResource
import org.jetbrains.compose.resources.painterResource
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
        remember(animatedPositionState) { { animatedPositionState.intValue } }
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
            FlowingLightBackground(state = backgroundState, modifier = Modifier.fillMaxSize())
            val artworkData by
            remember(uiStateState) { derivedStateOf { uiStateState.value.artworkData } }

            val layoutType = LocalWindowLayoutType.current
            when (layoutType) {
                WindowLayoutType.Phone -> {
                    val currentMusicItem by
                    remember(uiStateState) {
                        derivedStateOf { uiStateState.value.currentMusicItem }
                    }
                    val showTranslation by
                    remember(uiStateState) {
                        derivedStateOf { uiStateState.value.showTranslation }
                    }
                    val showPhonetic by
                    remember(uiStateState) {
                        derivedStateOf { uiStateState.value.showPhonetic }
                    }
                    val lyrics by
                    remember(uiStateState) { derivedStateOf { uiStateState.value.lyrics } }

                    MobilePlayerScreen(
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
                }

                else -> {
                    val currentMusicItem by
                    remember(uiStateState) {
                        derivedStateOf { uiStateState.value.currentMusicItem }
                    }
                    val showTranslation by
                    remember(uiStateState) {
                        derivedStateOf { uiStateState.value.showTranslation }
                    }
                    val showPhonetic by
                    remember(uiStateState) {
                        derivedStateOf { uiStateState.value.showPhonetic }
                    }
                    val lyrics by
                    remember(uiStateState) { derivedStateOf { uiStateState.value.lyrics } }

                    PadPlayerScreen(
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
                }
            }

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

@Composable
fun MobilePlayerScreen(
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
    Column {
        Row(
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
            modifier =
                Modifier
                    .captionBarPadding()
                    .statusBarsPadding()
                    .padding(horizontal = 28.dp)
                    .padding(top = 28.dp)
                    .fillMaxWidth(),
        ) {
            Row(
                Modifier.weight(1f),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (artworkData != null) {
                    PlayerArtwork(
                        artworkData,
                        imageResource(Res.drawable.empty),
                        Modifier
                            .clip(ContinuousRoundedRectangle(12.dp))
                            .size(72.dp),
                    )
                }
                PlayerMetadata(
                    currentMusicItem?.label ?: "Unknown Title",
                    currentMusicItem?.artist ?: "Unknown",
                )
            }
            Spacer(Modifier.width(8.dp))
            PlayerControls(
                onOpenSongSelection = { playerViewModel.onOpenSongSelection() },
                showTranslation = showTranslation,
                showPhonetic = showPhonetic,
                onToggleTranslation = { playerViewModel.toggleTranslation() },
                onTogglePhonetic = { playerViewModel.togglePhonetic() },
            )
        }

        val cover = (backgroundState.bitmap ?: imageResource(Res.drawable.empty)).asAndroidBitmap()
        PlayerLyrics(
            listState = listState,
            lyrics = lyrics,
            currentPosition = animatedPosition,
            showTranslation = showTranslation,
            showPhonetic = showPhonetic,
            onSeekTo = { playerViewModel.seekTo(it) },
            onShare = { line ->
                lyrics?.let { lyrics ->
                    playerViewModel.onShareRequested()
                    val context =
                        ShareContext(
                            lyrics = lyrics,
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
            modifier = Modifier.padding(horizontal = 12.dp),
        )
    }
}

@Composable
fun PadPlayerScreen(
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
    Row(
        Modifier
            .captionBarPadding()
            .statusBarsPadding()
            .fillMaxWidth()
            .animateContentSize(),
        horizontalArrangement = Arrangement.Center,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
            modifier =
                Modifier
                    .fillMaxWidth(0.4f)
                    .fillMaxHeight()
                    .padding(start = 100.dp)
                    .padding(top = 28.dp),
        ) {
            PlayerArtwork(
                artworkData,
                imageResource(Res.drawable.empty),
                Modifier
                    //                        .dropShadow(ContinuousRoundedRectangle(12.dp)) {
                    //                            radius = 10f
                    //                            color = Color.Black.copy(0.2f)
                    //                            offset = Offset(0f, 16f)
                    //                            spread = -10f
                    //                        }
                    .clip(ContinuousRoundedRectangle(12.dp))
                    .fillMaxWidth()
                    .aspectRatio(1f),
            )
            Spacer(
                Modifier
                    .fillMaxWidth()
                    .height(28.dp)
            )
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                PlayerMetadata(
                    currentMusicItem?.label ?: "Unknown Title",
                    currentMusicItem?.artist ?: "Unknown",
                )
                PlayerControls(
                    onOpenSongSelection = { playerViewModel.onOpenSongSelection() },
                    showTranslation = showTranslation,
                    showPhonetic = showPhonetic,
                    onToggleTranslation = { playerViewModel.toggleTranslation() },
                    onTogglePhonetic = { playerViewModel.togglePhonetic() },
                )
            }
        }
        AnimatedVisibility(lyrics != null) {
            val cover =
                (backgroundState.bitmap ?: imageResource(Res.drawable.empty)).asAndroidBitmap()
            PlayerLyrics(
                anchor = LyricsAnchor.Fraction(0.4f),
                bottomFade = LyricsFade.Fraction(0.2f),
                listState = listState,
                lyrics = lyrics,
                currentPosition = animatedPosition,
                showTranslation = showTranslation,
                showPhonetic = showPhonetic,
                onSeekTo = { playerViewModel.seekTo(it) },
                onShare = { line ->
                    lyrics?.let { lyrics ->
                        playerViewModel.onShareRequested()
                        val context =
                            ShareContext(
                                lyrics = lyrics,
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
                modifier =
                    Modifier
                        .padding(horizontal = 12.dp)
                        .padding(start = 60.dp, end = 60.dp)
                        .weight(1f),
            )
        }
    }
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
    AlertDialog(
        onDismissRequest = onDismissRequest,
        title = { Text("Open local files") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(
                    onClick = { selectAudio() },
                    enabled = !isImporting,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(audioName)
                }
                OutlinedButton(
                    onClick = { lyricsLauncher.launch(arrayOf("*/*")) },
                    enabled = !isImporting,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(lyricsName)
                }
                OutlinedButton(
                    onClick = { translationLauncher.launch(arrayOf("*/*")) },
                    enabled = !isImporting,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(translationName)
                }
                if (!storageAccess)
                    Text("Allow file access to automatically match lyrics beside the audio file.")
                error?.let {
                    Text(it, color = androidx.compose.material3.MaterialTheme.colorScheme.error)
                }
            }
        },
        confirmButton = {
            Button(
                onClick = { audioUri?.let { onSongSelected(it, lyricsUri, translationUri) } },
                enabled = audioUri != null && isReady && !isImporting,
            ) {
                Text(if (isImporting) "Opening…" else "Play")
            }
        },
        dismissButton = { Button(onClick = onDismissRequest) { Text("Cancel") } },
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
fun PlayerMetadata(title: String, artist: String, modifier: Modifier = Modifier) {
    Column(
        modifier = Modifier
            .graphicsLayer {
                blendMode = BlendMode.Plus
                compositingStrategy = CompositingStrategy.Offscreen
            }
    ) {
        Text(
            text = title,
            style = TextStyle(
                fontSize = 16.sp,
                lineHeight = 16.sp,
                fontWeight = FontWeight.SemiBold
            ),
            color = Color.White,
            modifier = Modifier
                .basicMarquee(spacing = MarqueeSpacing(20.dp), repeatDelayMillis = 2000),
        )
        Text(
            text = artist,
            style = TextStyle(
                fontSize = 15.sp,
                lineHeight = 15.sp,
                fontWeight = FontWeight.Medium
            ),
            modifier = Modifier
                .alpha(0.4f)
                .basicMarquee(spacing = MarqueeSpacing(20.dp), repeatDelayMillis = 2000),
            color = Color.White,
        )
    }
}

@Composable
fun PlayerControls(
    onOpenSongSelection: () -> Unit,
    showTranslation: Boolean,
    showPhonetic: Boolean,
    onToggleTranslation: () -> Unit,
    onTogglePhonetic: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier.graphicsLayer { blendMode = BlendMode.Plus },
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Box(
            Modifier
                .clip(CircleShape)
                .background(Color.White.copy(if (showTranslation) 0.6f else 0.2f))
                .clickable(onClick = onToggleTranslation)
                .padding(4.dp)
        ) {
            Icon(
                painterResource(Res.drawable.ic_translation),
                null,
                Modifier
                    .size(20.dp)
                    .align(Alignment.Center)
                    .graphicsLayer {
                        if (showTranslation) {
                            blendMode = BlendMode.DstOut
                        }
                    },
                tint = Color.White,
            )
        }

        Box(
            Modifier
                .clip(CircleShape)
                .background(Color.White.copy(if (showPhonetic) 0.6f else 0.2f))
                .clickable(onClick = onTogglePhonetic)
                .padding(4.dp)
        ) {
            Icon(
                painterResource(Res.drawable.ic_phonetic),
                null,
                Modifier
                    .size(20.dp)
                    .align(Alignment.Center)
                    .graphicsLayer {
                        if (showPhonetic) {
                            blendMode = BlendMode.DstOut
                        }
                    },
                tint = Color.White,
            )
        }

        Box(
            Modifier
                .clip(CircleShape)
                .background(Color.White.copy(0.2f))
                .clickable(onClick = onOpenSongSelection)
                .padding(4.dp)
        ) {
            Icon(
                painterResource(Res.drawable.ic_ellipsis),
                null,
                Modifier
                    .size(20.dp)
                    .align(Alignment.Center),
                tint = Color.White,
            )
        }
    }
}

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
    if (lyrics == null) {
        Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Text(
                "No timed lyrics loaded. Open local audio and lyrics to begin.",
                color = Color.White.copy(alpha = 0.6f),
                modifier = Modifier.padding(24.dp),
            )
        }
        return
    }

    val currentTextStyle = LocalTextStyle.current
    val sf = SFPro()
    val normalStyle =
        remember(currentTextStyle, sf) {
            currentTextStyle.copy(
                fontSize = 34.sp,
                lineHeight = androidx.compose.ui.unit.TextUnit.Unspecified,
                fontFamily = sf,
                fontWeight = FontWeight.Bold,
                textMotion = TextMotion.Animated,
            )
        }

    val accompanimentStyle =
        remember(currentTextStyle, sf) {
            currentTextStyle.copy(
                fontSize = 20.sp,
                lineHeight = androidx.compose.ui.unit.TextUnit.Unspecified,
                fontFamily = sf,
                fontWeight = FontWeight.Bold,
                textMotion = TextMotion.Animated,
            )
        }

    KaraokeLyricsView(
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
        normalLineTextStyle = normalStyle,
        accompanimentLineTextStyle = accompanimentStyle,
        modifier = modifier,
        useBlurEffect = true,
    )
}
