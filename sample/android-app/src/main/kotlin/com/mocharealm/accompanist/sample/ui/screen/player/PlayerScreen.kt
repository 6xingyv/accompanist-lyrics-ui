package com.mocharealm.accompanist.sample.ui.screen.player

import android.app.Activity
import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.Image
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextMotion
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.mapper.toKaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.KaraokeLyricsView
import com.mocharealm.accompanist.sample.Res
import com.mocharealm.accompanist.sample.empty
import com.mocharealm.accompanist.sample.ic_ellipsis
import com.mocharealm.accompanist.sample.sf_pro
import com.mocharealm.accompanist.sample.ui.adaptive.LocalWindowLayoutType
import com.mocharealm.accompanist.sample.ui.adaptive.WindowLayoutType
import com.mocharealm.accompanist.sample.ui.composable.ModalScaffold
import com.mocharealm.accompanist.sample.ui.composable.background.FlowingLightBackground
import com.mocharealm.accompanist.sample.ui.screen.share.ShareContext
import com.mocharealm.accompanist.sample.ui.screen.share.ShareScreen
import com.mocharealm.accompanist.sample.ui.screen.share.ShareViewModel
import com.mocharealm.accompanist.sample.ui.theme.SFPro
import com.mocharealm.gaze.capsule.ContinuousRoundedRectangle
import kotlinx.coroutines.android.awaitFrame
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.imageResource
import org.jetbrains.compose.resources.painterResource
import org.koin.compose.viewmodel.koinViewModel
import java.io.File
import kotlin.math.abs


@Composable
fun PlayerScreen(
    playerViewModel: PlayerViewModel = koinViewModel(),
    shareViewModel: ShareViewModel = koinViewModel(),
) {
    val animatedPositionState = remember { mutableLongStateOf(0L) }

    val currentPositionProvider = remember {
        { animatedPositionState.longValue.toInt() }
    }

    val uiState by playerViewModel.uiState.collectAsState()
    val latestPlaybackState by rememberUpdatedState(uiState.playbackState)

    LaunchedEffect(latestPlaybackState.isPlaying) {
        if (latestPlaybackState.isPlaying) {
            while (true) {
                val elapsed = System.currentTimeMillis() - latestPlaybackState.lastUpdateTime
                val newPosition = (latestPlaybackState.position + elapsed).coerceAtMost(
                    latestPlaybackState.duration
                )

                val currentAnimPos = animatedPositionState.longValue

                if (currentAnimPos <= newPosition || abs(newPosition - currentAnimPos) >= 100) {
                    animatedPositionState.longValue = newPosition
                }
                awaitFrame()
            }
        } else {
            animatedPositionState.longValue = latestPlaybackState.position
        }
    }

    Box(modifier = Modifier.fillMaxSize()) {
        ModalScaffold(
            isModalOpen = uiState.isShareSheetVisible,
            modifier = Modifier.fillMaxSize(),
            onDismissRequest = {
                playerViewModel.onShareDismissed()
                shareViewModel.reset()
            },
            modalContent = {
                ShareScreen(it, shareViewModel = shareViewModel)
            }
        ) {
            FlowingLightBackground(
                state = uiState.backgroundState,
                modifier = Modifier.fillMaxSize()
            )

            when (LocalWindowLayoutType.current) {
                WindowLayoutType.Phone -> {
                    MobilePlayerScreen(
                        currentPositionProvider, // 3. 传入 Provider
                        playerViewModel,
                        shareViewModel,
                        uiState
                    )
                }

                else -> {
                    PadPlayerScreen(
                        currentPositionProvider, // 3. 传入 Provider
                        playerViewModel,
                        shareViewModel,
                        uiState
                    )
                }
            }

            if (uiState.showSelectionDialog) {
                SongSelectionDialog(
                    onSongSelected = { audioUri, lyricsUri, translationUri ->
                        playerViewModel.onSongSelected(audioUri, lyricsUri, translationUri)
                    },
                    onSelectionChanged = { audioUri, lyricsUri, translationUri ->
                        playerViewModel.prepareSongSelection(audioUri, lyricsUri, translationUri)
                    },
                    findExternalLyricsPath = { uri -> playerViewModel.findExternalLyricsPath(uri) },
                    isPreparingSelection = uiState.isPreparingSelection,
                    isSelectionPrepared = uiState.currentMusicItem != null,
                    onDismissRequest = { /* Optionally handle dismiss */ }
                )
            }
        }
    }
}

@Composable
fun MobilePlayerScreen(
    animatedPosition: () -> Int,
    playerViewModel: PlayerViewModel,
    shareViewModel: ShareViewModel,
    uiState: PlayerUiState
) {
    Column {
        Row(
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier
                .captionBarPadding()
                .statusBarsPadding()
                .padding(horizontal = 28.dp)
                .padding(top = 28.dp)
                .fillMaxWidth()
        ) {
            Row(
                Modifier.weight(1f),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                uiState.backgroundState.bitmap?.let { bitmap ->
                    Image(
                        bitmap,
                        null,
                        Modifier
                            .clip(ContinuousRoundedRectangle(6.dp))
                            .border(
                                1.dp,
                                Color.White.copy(0.2f),
                                ContinuousRoundedRectangle(6.dp)
                            )
                            .size(60.dp)
                    )
                }
                PlayerMetadata(
                    uiState.currentMusicItem?.label ?: "Unknown Title",
                    uiState.currentMusicItem?.artist ?: "Unknown"
                )
            }
            Spacer(Modifier.width(8.dp))
            PlayerControls(onOpenSongSelection = { playerViewModel.onOpenSongSelection() })
        }

        val cover =
            (uiState.backgroundState.bitmap ?: imageResource(Res.drawable.empty)).asAndroidBitmap()
        PlayerLyrics(
            lyrics = uiState.lyrics,
            currentPosition = animatedPosition,
            onSeekTo = { playerViewModel.seekTo(it) },
            onShare = { line ->
                uiState.lyrics?.let { lyrics ->
                    playerViewModel.onShareRequested()
                    val context = ShareContext(
                        lyrics = lyrics.copy(lines = lyrics.lines.map { line ->
                            when (line) {
                                is KaraokeLine -> line
                                is SyncedLine -> line.toKaraokeLine()
                                else -> null
                            }
                        }.filterNotNull()),
                        initialLine = line,
                        backgroundState = uiState.backgroundState,
                        title = uiState.currentMusicItem?.label ?: "Unknown Title",
                        artist = uiState.currentMusicItem?.artist ?: "Unknown",
                        cover = cover
                    )
                    shareViewModel.prepareForSharing(context)
                    playerViewModel.onShareRequested()
                }
            },
            modifier = Modifier.padding(horizontal = 12.dp)
        )
    }
}

@Composable
fun PadPlayerScreen(
    animatedPosition: () -> Int,
    playerViewModel: PlayerViewModel,
    shareViewModel: ShareViewModel,
    uiState: PlayerUiState
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
            modifier = Modifier
                .fillMaxWidth(0.4f)
                .fillMaxHeight()
                .padding(start = 100.dp)
                .padding(top = 28.dp)
        ) {
            Image(
                uiState.backgroundState.bitmap ?: imageResource(Res.drawable.empty),
                null,
                Modifier
//                        .dropShadow(ContinuousRoundedRectangle(12.dp)) {
//                            radius = 10f
//                            color = Color.Black.copy(0.2f)
//                            offset = Offset(0f, 16f)
//                            spread = -10f
//                        }
                    .border(
                        1.dp,
                        Color.White.copy(0.2f),
                        ContinuousRoundedRectangle(12.dp)
                    )
                    .clip(ContinuousRoundedRectangle(12.dp))
                    .fillMaxWidth()
                    .aspectRatio(1f)
            )
            Spacer(
                Modifier
                    .fillMaxWidth()
                    .height(28.dp)
            )
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                PlayerMetadata(
                    uiState.currentMusicItem?.label ?: "Unknown Title",
                    uiState.currentMusicItem?.artist ?: "Unknown"
                )
                PlayerControls(onOpenSongSelection = { playerViewModel.onOpenSongSelection() })
            }

        }
        AnimatedVisibility(uiState.lyrics != null) {
            val cover =
                (uiState.backgroundState.bitmap
                    ?: imageResource(Res.drawable.empty)).asAndroidBitmap()
            PlayerLyrics(
                lyrics = uiState.lyrics,
                currentPosition = animatedPosition,
                onSeekTo = { playerViewModel.seekTo(it) },
                onShare = { line ->
                    uiState.lyrics?.let { lyrics ->
                        playerViewModel.onShareRequested()
                        val context = ShareContext(
                            lyrics = lyrics.copy(lines = lyrics.lines.map { line ->
                                when (line) {
                                    is KaraokeLine -> line
                                    is SyncedLine -> line.toKaraokeLine()
                                    else -> null
                                }
                            }.filterNotNull()),
                            initialLine = line,
                            backgroundState = uiState.backgroundState,
                            title = uiState.currentMusicItem?.label ?: "Unknown Title",
                            artist = uiState.currentMusicItem?.artist ?: "Unknown",
                            cover = cover
                        )
                        shareViewModel.prepareForSharing(context)
                        playerViewModel.onShareRequested()
                    }
                },
                modifier = Modifier
                    .padding(horizontal = 12.dp)
                    .padding(start = 60.dp, end = 60.dp)
                    .weight(1f)
            )
        }
    }
}

@Composable
fun SongSelectionDialog(
    onSongSelected: (Uri, Uri?, Uri?) -> Unit,
    onSelectionChanged: (Uri, Uri?, Uri?) -> Unit,
    findExternalLyricsPath: suspend (Uri) -> String?,
    isPreparingSelection: Boolean,
    isSelectionPrepared: Boolean,
    onDismissRequest: () -> Unit
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    var hasAllFilesAccess by remember { mutableStateOf(hasAllFilesAccess()) }
    var audioUri by remember { mutableStateOf<Uri?>(null) }
    var lyricsUri by remember { mutableStateOf<Uri?>(null) }
    var translationUri by remember { mutableStateOf<Uri?>(null) }

    var audioName by remember { mutableStateOf("Select Audio") }
    var lyricsName by remember { mutableStateOf("Select Lyrics") }
    var translationName by remember { mutableStateOf("Select Translation (Optional)") }

    val allFilesAccessLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
            hasAllFilesAccess = hasAllFilesAccess()
        }

    fun launchAllFilesAccessSettings() {
        val packageSettingsIntent = Intent(
            Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
            Uri.parse("package:${context.packageName}")
        )
        val fallbackIntent = Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION)
        runCatching { allFilesAccessLauncher.launch(packageSettingsIntent) }
            .onFailure { allFilesAccessLauncher.launch(fallbackIntent) }
    }

    val audioLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            val uri = result.data?.data
            if (result.resultCode == Activity.RESULT_OK && uri != null) {
                val selectedAudioUri = uri
                audioUri = selectedAudioUri
                audioName = displayNameForUri(context, selectedAudioUri, "Audio Selected")
                lyricsUri = null
                lyricsName = "Select Lyrics"
                translationUri = null
                translationName = "Select Translation (Optional)"
                onSelectionChanged(selectedAudioUri, null, null)

                scope.launch {
                    val detectedLyricsPath = findExternalLyricsPath(selectedAudioUri)
                    if (audioUri == selectedAudioUri && lyricsUri == null && detectedLyricsPath != null) {
                        lyricsUri = Uri.fromFile(File(detectedLyricsPath))
                        lyricsName = File(detectedLyricsPath).name
                    }
                }
            }
        }

    val lyricsLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            val uri = result.data?.data
            if (result.resultCode == Activity.RESULT_OK && uri != null) {
                lyricsUri = uri
                lyricsName = displayNameForUri(context, uri, "Lyrics Selected")
                audioUri?.let { onSelectionChanged(it, lyricsUri, translationUri) }
            }
        }

    val translationLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            val uri = result.data?.data
            if (result.resultCode == Activity.RESULT_OK && uri != null) {
                translationUri = uri
                translationName = displayNameForUri(context, uri, "Translation Selected")
                audioUri?.let { onSelectionChanged(it, lyricsUri, translationUri) }
            }
        }

    AlertDialog(
        onDismissRequest = onDismissRequest,
        title = { Text("Choose a song to play") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (!hasAllFilesAccess) {
                    Button(
                        onClick = ::launchAllFilesAccessSettings,
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Text("Grant File Access")
                    }
                }
                OutlinedButton(
                    onClick = {
                        audioLauncher.launch(
                            Intent(Intent.ACTION_GET_CONTENT).apply {
                                type = "audio/*"
                                addCategory(Intent.CATEGORY_OPENABLE)
                            }
                        )
                    },
                    modifier = Modifier.fillMaxWidth(),
                    enabled = hasAllFilesAccess
                ) {
                    Text(audioName)
                }
                OutlinedButton(
                    onClick = {
                        lyricsLauncher.launch(
                            Intent(Intent.ACTION_GET_CONTENT).apply {
                                type = "*/*"
                                addCategory(Intent.CATEGORY_OPENABLE)
                            }
                        )
                    },
                    modifier = Modifier.fillMaxWidth(),
                    enabled = hasAllFilesAccess
                ) {
                    Text(lyricsName)
                }
                OutlinedButton(
                    onClick = {
                        translationLauncher.launch(
                            Intent(Intent.ACTION_GET_CONTENT)
                                .setType("*/*")
                                .addCategory(Intent.CATEGORY_OPENABLE)
                        )
                    },
                    modifier = Modifier.fillMaxWidth(),
                    enabled = hasAllFilesAccess
                ) {
                    Text(translationName)
                }
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    audioUri?.let { onSongSelected(it, lyricsUri, translationUri) }
                },
                enabled = hasAllFilesAccess && audioUri != null && isSelectionPrepared && !isPreparingSelection
            ) {
                Text(if (isPreparingSelection) "Parsing..." else "Play")
            }
        },
        dismissButton = {
            Button(onClick = onDismissRequest) {
                Text("Cancel")
            }
        }
    )
}

private fun hasAllFilesAccess(): Boolean =
    Build.VERSION.SDK_INT < Build.VERSION_CODES.R || Environment.isExternalStorageManager()

private fun displayNameForUri(context: Context, uri: Uri, fallback: String): String {
    if (uri.scheme == ContentResolver.SCHEME_FILE) {
        return uri.path?.let { File(it).name } ?: fallback
    }

    return runCatching {
        context.contentResolver.query(
            uri,
            arrayOf(OpenableColumns.DISPLAY_NAME),
            null,
            null,
            null
        )?.use { cursor ->
            if (cursor.moveToFirst()) {
                val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (index >= 0) cursor.getString(index) else null
            } else {
                null
            }
        }
    }.getOrNull()
        ?: uri.lastPathSegment?.substringAfterLast('/')
        ?: fallback
}

@Composable
fun PlayerMetadata(
    title: String,
    artist: String,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = Modifier.graphicsLayer {
            blendMode = BlendMode.Plus
            compositingStrategy = CompositingStrategy.Offscreen
        }
    ) {
        Text(
            text = title,
            style = LocalTextStyle.current.copy(
                fontWeight = FontWeight.Bold,
                textMotion = TextMotion.Animated
            ),
            color = Color.White,
            modifier = Modifier.basicMarquee(
                spacing = MarqueeSpacing(20.dp),
                repeatDelayMillis = 2000
            )
        )
        Text(
            text = artist,
            modifier = Modifier
                .alpha(0.4f)
                .basicMarquee(
                    spacing = MarqueeSpacing(20.dp),
                    repeatDelayMillis = 2000
                ),
            style = LocalTextStyle.current.copy(
                textMotion = TextMotion.Animated
            ),
            lineHeight = 1.em,
            color = Color.White
        )
    }
}

@Composable
fun PlayerControls(
    onOpenSongSelection: () -> Unit,
    modifier: Modifier = Modifier
) {
    Row(
        modifier = modifier.graphicsLayer { blendMode = BlendMode.Plus },
        horizontalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        Box(
            Modifier
                .clip(CircleShape)
                .background(Color.White.copy(0.2f))
                .clickable(onClick = onOpenSongSelection)
                .padding(4.dp),
        ) {
            Icon(
                painterResource(Res.drawable.ic_ellipsis),
                null,
                Modifier
                    .size(20.dp)
                    .align(Alignment.Center),
                tint = Color.White
            )
        }
    }
}

@Composable
fun PlayerLyrics(
    lyrics: SyncedLyrics?,
    currentPosition: () -> Int,
    onSeekTo: (Int) -> Unit,
    onShare: (KaraokeLine) -> Unit,
    modifier: Modifier = Modifier
) {
    if (lyrics == null) return

    KaraokeLyricsView(
        lyrics = lyrics,
        currentPosition = currentPosition,
        onLineClicked = { line ->
            onSeekTo(line.start)
        },
        onLinePressed = { line ->
            val karaokeLine = when (line) {
                is KaraokeLine -> line
                is SyncedLine -> line.toKaraokeLine()
                else -> null
            }
            karaokeLine?.let {
                onShare(it)
            }
        },
        modifier = modifier.graphicsLayer {
            compositingStrategy = CompositingStrategy.Offscreen
            blendMode = BlendMode.Plus
        },
        fontResource = Res.font.sf_pro
    )
}
