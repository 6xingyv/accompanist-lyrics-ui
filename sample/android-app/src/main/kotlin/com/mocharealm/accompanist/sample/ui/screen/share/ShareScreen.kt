package com.mocharealm.accompanist.sample.ui.screen.share

import android.annotation.SuppressLint
import android.content.Intent
import android.graphics.Bitmap
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.PagerState
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextMotion
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.sample.Res
import com.mocharealm.accompanist.sample.ic_accompanist
import com.mocharealm.accompanist.sample.ui.composable.background.BackgroundVisualState
import com.mocharealm.accompanist.sample.ui.composable.background.FlowingLightBackground
import com.mocharealm.accompanist.sample.ui.screen.player.PlayerMetadata
import com.mocharealm.accompanist.sample.ui.utils.composable.Capturable
import com.mocharealm.accompanist.sample.ui.utils.composable.CapturableController
import com.mocharealm.accompanist.sample.ui.utils.composable.rememberCapturableController
import com.mocharealm.gaze.capsule.ContinuousRoundedRectangle
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.painterResource
import org.koin.compose.viewmodel.koinViewModel
import kotlin.math.absoluteValue

fun KaraokeLine.content(): String = this.syllables.joinToString("") { it.content }

@SuppressLint("ModifierParameter")
@Composable
fun ShareScreen(
    dragModifier: Modifier,
    shareViewModel: ShareViewModel = koinViewModel()
) {
    val uiState by shareViewModel.uiState.collectAsStateWithLifecycle()
    val context = uiState.context ?: return // If context isn't ready, don't render anything

    Column(
        Modifier
            .navigationBarsPadding()
            .clickable( // Absorb clicks to prevent dismissing the modal by clicking content
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = {}
            )
    ) {
        Box(
            dragModifier
                .fillMaxWidth()
                .padding(16.dp),
        ) {
            Text(
                text = "Share",
                Modifier.align(Alignment.Center),
                color = if (isSystemInDarkTheme()) Color.White else Color.Black
            )
        }
        Spacer(
            Modifier
                .fillMaxWidth()
                .height(0.5.dp)
                .background(
                    if (isSystemInDarkTheme()) Color.White.copy(0.05f) else Color.Black.copy(0.2f)
                )
        )
        AnimatedContent(uiState.step, transitionSpec = {
            fadeIn() togetherWith fadeOut()
        }) { step ->
            when (step) {
                ShareStep.SELECTING -> {
                    ShareSelectionStep(
                        context = context,
                        selectedLines = uiState.selectedLines,
                        onLineToggled = { shareViewModel.toggleLineSelection(it) },
                        onGenerateClicked = { shareViewModel.proceedToGenerate() }
                    )
                }

                ShareStep.GENERATING -> {
                    ShareGenerateStep(
                        context = context,
                        selectedLines = uiState.selectedLines,
                        onBackPressed = { shareViewModel.returnToSelection() },
                        shareViewModel = shareViewModel
                    )
                }
            }
        }
    }
}

@Composable
private fun ShareSelectionStep(
    context: ShareContext,
    selectedLines: List<KaraokeLine>,
    onLineToggled: (KaraokeLine) -> Unit,
    onGenerateClicked: () -> Unit
) {
    val shareLazyState = rememberLazyListState()
    val coroutineScope = rememberCoroutineScope()

    LaunchedEffect(selectedLines.first()) {
        coroutineScope.launch {
            val index = context.lyrics.lines.indexOf(selectedLines.first())
            if (index != -1) {
                shareLazyState.animateScrollToItem(index, -200)
            }
        }
    }
    Column {
        LazyColumn(
            state = shareLazyState,
            modifier = Modifier
                .weight(1f)
                .background(
                    if (isSystemInDarkTheme()) Color.White.copy(0.1f) else Color.Black.copy(0.02f)
                ),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            stickyHeader {
                Row {

                }
            }
            items(context.lyrics.lines, key = { it.start }) { line ->
                val karaokeLine = line as KaraokeLine
                val isSelected = selectedLines.contains(karaokeLine)
                val backgroundColor by animateColorAsState(
                    if (isSelected) Color(0xFF3482FF).copy(0.2f)
                    else if (selectedLines.size == 5) Color.Black.copy(0.2f)
                    else if (isSystemInDarkTheme()) Color.White.copy(0.1f) else Color.White
                )
                Column(
                    Modifier
                        .clip(
                            if (selectedLines.size > 1)
                                when {
                                    selectedLines.first() == line -> ContinuousRoundedRectangle(
                                        16.dp,
                                        16.dp,
                                        8.dp,
                                        8.dp
                                    )

                                    selectedLines.last() == line -> ContinuousRoundedRectangle(
                                        8.dp,
                                        8.dp,
                                        16.dp,
                                        16.dp
                                    )

                                    else -> ContinuousRoundedRectangle(8.dp)
                                }
                            else ContinuousRoundedRectangle(16.dp)
                        )
                        .fillMaxWidth()
                        .clickable { onLineToggled(karaokeLine) }
                        .background(
                            backgroundColor
                        )
                        .padding(16.dp)
                ) {
                    Text(
                        karaokeLine.content(),
                        style = LocalTextStyle.current.copy(
                            fontSize = 24.sp,
                            fontWeight = FontWeight.Bold,
                            textMotion = TextMotion.Animated
                        ),
                        color = if (isSystemInDarkTheme()) Color.White else Color.Black
                    )
                    karaokeLine.translation?.let { translation ->
                        Text(
                            translation, Modifier.alpha(0.6f),
                            color = if (isSystemInDarkTheme()) Color.White else Color.Black
                        )
                    }
                }
            }
        }
        Column {
            Spacer(
                Modifier
                    .fillMaxWidth()
                    .height(0.5.dp)
                    .background(
                        if (isSystemInDarkTheme()) Color.White.copy(0.2f) else Color.Black.copy(0.2f)
                    )
            )
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.SpaceEvenly
            ) {
                Box(
                    Modifier
                        .weight(1f)
                        .clip(ContinuousRoundedRectangle(100))
                        .background(Color(0xFF3482FF))
                        .clickable(enabled = selectedLines.isNotEmpty()) { onGenerateClicked() }
                        .padding(16.dp),
                    contentAlignment = Alignment.Center
                ) {
                    Text("Generate Share Card", color = Color.White)
                }
            }
        }
    }
}

@Composable
private fun ShareGenerateStep(
    context: ShareContext,
    shareViewModel: ShareViewModel,
    selectedLines: List<KaraokeLine>,
    onBackPressed: () -> Unit,
) {
    Column {
        BackHandler { onBackPressed() }
        var showTranslation by remember { mutableStateOf(true) }
        val localContext = LocalContext.current
        LaunchedEffect(Unit) {
            shareViewModel.toastEvent.collect { message ->
                Toast.makeText(localContext, message, Toast.LENGTH_SHORT).show()
            }
        }

        // Listen for share events
        LaunchedEffect(Unit) {
            shareViewModel.shareEvent.collect { uri ->
                val sendIntent: Intent = Intent().apply {
                    action = Intent.ACTION_SEND
                    putExtra(Intent.EXTRA_STREAM, uri)
                    type = "image/png"
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                val shareIntent = Intent.createChooser(sendIntent, null)
                localContext.startActivity(shareIntent)
            }
        }

        val pagerState = rememberPagerState {
            2
        }
        val capturableControllers = List(pagerState.pageCount) {
            rememberCapturableController()
        }
        HorizontalPager(
            pagerState,
            Modifier
                .weight(1f)
                .background(
                    if (isSystemInDarkTheme()) Color.White.copy(0.1f) else Color.Black.copy(
                        0.1f
                    )
                ),
            beyondViewportPageCount = 2
        ) { pageNumber ->
            Box(
                Modifier
                    .fillMaxSize()
                    .pagerCubeInDepthTransition(pageNumber, pagerState)
            ) {
                when (pageNumber) {
                    0 -> {
                        ShareCardApple(
                            capturableController = capturableControllers[pageNumber],
                            backgroundState = context.backgroundState,
                            showTranslation = showTranslation,
                            selectedLines = selectedLines,
                            title = context.title,
                            artist = context.artist,
                            cover = context.cover,
                            modifier = Modifier.align(Alignment.Center)
                        )
                    }

                    1 -> {
                        ShareCardSpotify(
                            capturableController = capturableControllers[pageNumber],
                            showTranslation = showTranslation,
                            selectedLines = selectedLines,
                            modifier = Modifier.align(Alignment.Center)
                        )
                    }
                }
            }
        }

        // Settings
        Column {
            Spacer(
                Modifier
                    .fillMaxWidth()
                    .height(0.5.dp)
                    .background(
                        if (isSystemInDarkTheme()) Color.White.copy(0.2f) else Color.Black.copy(0.2f)
                    )
            )
            if (selectedLines.any { it.translation != null }) {
                Row(
                    Modifier
                        .fillMaxWidth()
                        .padding(16.dp),
                    Arrangement.SpaceBetween,
                    Alignment.CenterVertically
                ) {
                    Column {
                        Text(
                            "Show translation",
                            color = if (isSystemInDarkTheme()) Color.White else Color.Black
                        )
//                        Text(
//                            "Tap to toggle",
//                            color = if (isSystemInDarkTheme()) Color.White else Color.Black
//                        )
                    }
                    Switch(showTranslation, onCheckedChange = { showTranslation = it })
                }
                Spacer(
                    Modifier
                        .fillMaxWidth()
                        .height(0.5.dp)
                        .background(
                            if (isSystemInDarkTheme()) Color.White.copy(0.2f) else Color.Black.copy(
                                0.2f
                            )
                        )
                )
            }
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                Box(
                    Modifier
                        .weight(1f)
                        .clip(ContinuousRoundedRectangle(100))
                        .background(Color(0xFF3482FF))
                        .clickable(enabled = selectedLines.isNotEmpty()) {
                            capturableControllers[pagerState.currentPage].capture { bitmap ->
                                shareViewModel.saveBitmapToGallery(
                                    localContext,
                                    bitmap.asAndroidBitmap()
                                )
                            }
                        }
                        .padding(16.dp),
                    contentAlignment = Alignment.Center
                ) {
                    Text("Save", color = Color.White)
                }
                Box(
                    Modifier
                        .weight(1f)
                        .clip(ContinuousRoundedRectangle(100))
                        .background(Color(0xFF3482FF))
                        .clickable(enabled = selectedLines.isNotEmpty()) {
                            capturableControllers[pagerState.currentPage].capture { bitmap ->
                                shareViewModel.prepareForSharing(
                                    localContext,
                                    bitmap.asAndroidBitmap()
                                )
                            }
                        }
                        .padding(16.dp),
                    contentAlignment = Alignment.Center
                ) {
                    Text("Share", color = Color.White)
                }
            }
        }
    }
}

@Composable
fun ShareCardApple(
    capturableController: CapturableController,
    backgroundState: BackgroundVisualState,
    showTranslation: Boolean,
    selectedLines: List<KaraokeLine>,
    title: String,
    artist: String,
    cover: Bitmap,
    modifier: Modifier = Modifier
) {
    Box(
        modifier
            .sizeIn(maxWidth = 300.dp)
            .shadow(16.dp, ContinuousRoundedRectangle(16.dp))
    ) {
        Capturable(
            controller = capturableController,
        ) {
            Box(
                modifier = Modifier
                    .clip(ContinuousRoundedRectangle(16.dp))
                    .background(Color.White)
            ) {
                FlowingLightBackground(
                    state = backgroundState,
                    modifier = Modifier.matchParentSize()
                )
                LazyColumn(
                    modifier = Modifier
                ) {
                    item("logo") {
                        Row(
                            Modifier
                                .padding(16.dp)
                                .alpha(0.4f)
                                .graphicsLayer {
                                    blendMode = BlendMode.Plus
                                    compositingStrategy = CompositingStrategy.Offscreen
                                },
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Icon(
                                painterResource(Res.drawable.ic_accompanist),
                                null,
                                Modifier.size(32.dp),
                                tint = Color.White
                            )
                            Text(
                                "Accompanist",
                                style = LocalTextStyle.current.copy(
                                    fontSize = 16.sp,
                                    fontWeight = FontWeight.Bold,
                                    color = Color.White
                                )
                            )
                        }
                    }
                    items(selectedLines, key = { it.start }) { line ->
                        Column(
                            Modifier
                                .padding(horizontal = 16.dp)
                                .padding(bottom = 16.dp)
                                .graphicsLayer {
                                    blendMode = BlendMode.Plus
                                    compositingStrategy = CompositingStrategy.Offscreen
                                }
                        ) {
                            Text(
                                line.content(),
                                style = LocalTextStyle.current.copy(
                                    fontSize = 24.sp,
                                    fontWeight = FontWeight.Bold,
                                    textMotion = TextMotion.Animated
                                ),
                                color = Color.White
                            )
                            AnimatedVisibility(showTranslation) {
                                line.translation?.let { translation ->
                                    Text(
                                        translation,
                                        color = Color.White.copy(0.2f)
                                    )
                                }
                            }
                        }
                    }
                    item("info") {
                        Row(
                            Modifier
                                .background(Color.White.copy(0.1f))
                                .fillMaxWidth()
                                .padding(16.dp),
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Image(
                                cover.asImageBitmap(),
                                null,
                                Modifier
                                    .size(48.dp)
                                    .clip(ContinuousRoundedRectangle(4.dp))
                            )
                            PlayerMetadata(title, artist)
                        }
                    }
                }
            }
        }
    }
}

@Composable
fun ShareCardSpotify(
    capturableController: CapturableController,
    showTranslation: Boolean,
    selectedLines: List<KaraokeLine>,
    modifier: Modifier = Modifier
) {
    Box(
        modifier
            .fillMaxWidth(0.7f)
//            .deviceRotation()
            .shadow(16.dp)
    ) {
        Capturable(
            controller = capturableController
        ) {
            LazyColumn(
                modifier = Modifier
                    .background(Color(0xff34344c))
                    .graphicsLayer {
                        blendMode = BlendMode.Plus
                        compositingStrategy = CompositingStrategy.Offscreen
                    },
//                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                items(selectedLines, key = { it.start }) { line ->
                    Column(
                        Modifier
                            .padding(top = 16.dp)
                            .padding(horizontal = 16.dp)
                    ) {
                        Text(
                            line.content(),
                            style = LocalTextStyle.current.copy(
                                fontSize = 24.sp,
                                fontWeight = FontWeight.Bold,
                                textMotion = TextMotion.Animated
                            ),
                            color = Color.White.copy(0.4f)
                        )
                        AnimatedVisibility(showTranslation) {
                            line.translation?.let { translation ->
                                Text(
                                    translation,
                                    color = Color.White.copy(0.8f)
                                )
                            }
                        }
                    }
                }
                item("spacing") {
                    Spacer(
                        Modifier
                            .fillMaxWidth()
                            .height(16.dp)
                    )
                }
                item("info") {
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .background(Color.White.copy(0.1f))
                            .padding(16.dp)
                    ) {
                        Text(
                            "by Accompanist",
                            style = LocalTextStyle.current.copy(
                                fontSize = 16.sp,
                                textMotion = TextMotion.Animated
                            ),
                            color = Color.White
                        )
                    }
                }
            }
        }
    }
}

fun Modifier.pagerCubeInDepthTransition(page: Int, pagerState: PagerState) = graphicsLayer {
    // Set camera distance to prevent excessive clipping or distortion during 3D rotation
    cameraDistance = 32f

    // Get the current page's offset relative to the viewport center.
    // 0f means the page is at the center.
    // 1f means the page is one full page to the right.
    // -1f means the page is one full page to the left.
    val pageOffset = pagerState.getOffsetDistanceInPages(page)

    // Set rotation center to the card's own center (X=50%, Y=50%)
    transformOrigin = TransformOrigin(0.5f, 0.5f)

    // Calculate rotation angle directly based on page offset.
    // When page slides right (pageOffset becomes positive), card rotates inward (clockwise).
    // When page slides left (pageOffset becomes negative), card rotates outward (counterclockwise).
    rotationY = -90f * pageOffset


    // When card deviates from center, slightly shrink it to increase depth perception
    val scale = 1f - 0.2f * pageOffset.absoluteValue
    scaleX = scale
    scaleY = scale
}