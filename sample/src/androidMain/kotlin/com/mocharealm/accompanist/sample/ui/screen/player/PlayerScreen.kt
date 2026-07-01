package com.mocharealm.accompanist.sample.ui.screen.player

import android.net.Uri
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
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
import androidx.compose.runtime.mutableIntStateOf
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
import androidx.compose.ui.viewinterop.AndroidView
import androidx.media3.common.MediaItem
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.mapper.toKaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.KaraokeLyricsView
import com.mocharealm.accompanist.lyrics.ui.renderer.RustSkiaLyricsView
import com.mocharealm.accompanist.sample.Res
import com.mocharealm.accompanist.sample.domain.model.MusicItem
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
                MusicItemSelectionDialog(
                    items = uiState.availableSongs,
                    onItemSelected = { item ->
                        playerViewModel.onSongSelected(item)
                    },
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
                    uiState.currentMusicItem?.testTarget?.split(" [")?.get(0)
                        ?: "Unknown"
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
                        lyrics = lyrics,
                        initialLine = line,
                        backgroundState = uiState.backgroundState,
                        title = uiState.currentMusicItem?.label ?: "Unknown Title",
                        artist = uiState.currentMusicItem?.testTarget?.split(" [")?.get(0)
                            ?: "Unknown",
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
                    uiState.currentMusicItem?.testTarget?.split(" [")?.get(0)
                        ?: "Unknown"
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
                            lyrics = lyrics,
                            initialLine = line,
                            backgroundState = uiState.backgroundState,
                            title = uiState.currentMusicItem?.label ?: "Unknown Title",
                            artist = uiState.currentMusicItem?.testTarget?.split(" [")?.get(0)
                                ?: "Unknown",
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
fun CustomSongSelectionDialog(
    onDismissRequest: () -> Unit,
    onSongSelected: (MusicItem) -> Unit
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    var audioUri by remember { mutableStateOf<Uri?>(null) }
    var lyricsUri by remember { mutableStateOf<Uri?>(null) }
    var translationUri by remember { mutableStateOf<Uri?>(null) }

    var audioName by remember { mutableStateOf("Select Audio") }
    var lyricsName by remember { mutableStateOf("Select Lyrics") }
    var translationName by remember { mutableStateOf("Select Translation (Optional)") }

    val audioLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
            uri?.let {
                audioUri = it
                audioName = it.path?.split("/")?.last() ?: "Audio Selected"
            }
        }

    val lyricsLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
            uri?.let {
                lyricsUri = it
                lyricsName = it.path?.split("/")?.last() ?: "Lyrics Selected"
            }
        }

    val translationLauncher =
        rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
            uri?.let {
                translationUri = it
                translationName = it.path?.split("/")?.last() ?: "Translation Selected"
            }
        }

    AlertDialog(
        onDismissRequest = onDismissRequest,
        title = { Text("Custom Song Selection") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(
                    onClick = { audioLauncher.launch("audio/*") },
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Text(audioName)
                }
                OutlinedButton(
                    onClick = { lyricsLauncher.launch("*/*") },
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Text(lyricsName)
                }
                OutlinedButton(
                    onClick = { translationLauncher.launch("*/*") },
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Text(translationName)
                }
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    if (audioUri != null && lyricsUri != null) {
                        scope.launch {
                            val lyricsContent = context.contentResolver.openInputStream(lyricsUri!!)
                                ?.bufferedReader()?.use { it.readText() } ?: ""
                            val translationContent = translationUri?.let {
                                context.contentResolver.openInputStream(it)?.bufferedReader()
                                    ?.use { it.readText() }
                            }

                            val item = MusicItem(
                                label = audioName,
                                testTarget = "Custom Selection",
                                mediaItem = MediaItem.fromUri(audioUri!!),
                                lyrics = lyricsContent,
                                translation = translationContent,
                                isCustom = true
                            )
                            onSongSelected(item)
                        }
                    }
                },
                enabled = audioUri != null && lyricsUri != null
            ) {
                Text("Play")
            }
        },
        dismissButton = {
            Button(onClick = { onDismissRequest() }) {
                Text("Cancel")
            }
        }
    )
}

@Composable
fun MusicItemSelectionDialog(
    items: List<MusicItem>,
    onItemSelected: (MusicItem) -> Unit,
    onDismissRequest: () -> Unit
) {
    var showCustomDialog by remember { mutableStateOf(false) }

    if (showCustomDialog) {
        CustomSongSelectionDialog(
            onDismissRequest = { showCustomDialog = false },
            onSongSelected = {
                showCustomDialog = false
                onItemSelected(it)
            }
        )
    } else {
        var selectedIndex by remember { mutableIntStateOf(-1) }
        AlertDialog(
            onDismissRequest = onDismissRequest,
            title = { Text("Choose a song to play") },
            text = {
                LazyColumn {
                    item {
                        OutlinedButton(
                            onClick = { showCustomDialog = true },
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(bottom = 8.dp)
                        ) {
                            Text("Select Custom File...")
                        }
                    }
                    itemsIndexed(items) { index, item ->
                        Column(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { selectedIndex = index }
                                .padding(vertical = 12.dp)) {
                            Text(item.label, fontWeight = FontWeight.Bold)
                            Text(item.testTarget, fontSize = 14.sp)
                        }
                    }
                }
            },
            confirmButton = {
                Text("Confirm", Modifier.clickable {
                    if (selectedIndex != -1) {
                        onItemSelected(items[selectedIndex])
                    }
                })
            })
    }
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
            run {
                when (line) {
                    is KaraokeLine -> line
                    is SyncedLine -> line.toKaraokeLine()
                    else -> null
                }
            }?.let(onShare)
        },
        modifier=modifier.graphicsLayer{
            compositingStrategy = CompositingStrategy.Offscreen
            blendMode = BlendMode.Plus
        }
    )
}
