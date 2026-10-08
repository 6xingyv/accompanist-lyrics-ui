package com.mocharealm.accompanist.sample.ui.screen.player

import android.content.ComponentName
import android.content.Context
import android.net.Uri
import android.os.SystemClock
import android.util.Log
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.unit.IntSize
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.media3.common.C
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.ListenableFuture
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.sample.data.repository.MusicRepositoryImpl
import com.mocharealm.accompanist.sample.domain.model.MusicItem
import com.mocharealm.accompanist.sample.domain.repository.LocalFileSelection
import com.mocharealm.accompanist.sample.domain.repository.MusicRepository
import com.mocharealm.accompanist.sample.service.PlaybackService
import com.mocharealm.accompanist.sample.ui.composable.background.BackgroundVisualState
import com.mocharealm.accompanist.sample.ui.playback.PlaybackSnapshot
import com.mocharealm.accompanist.sample.PlaybackTimingTrace
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.guava.await
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

data class PlayerUiState(
    val isReady: Boolean = false,
    val showSelectionDialog: Boolean = false,
    val backgroundState: BackgroundVisualState = BackgroundVisualState(null, 0f),
    val artworkData: ByteArray? = null,
    val lyrics: SyncedLyrics? = null,
    val isImporting: Boolean = false,
    val selectionError: String? = null,
    val currentMusicItem: MusicItem? = null,
    val isShareSheetVisible: Boolean = false,
    val showTranslation: Boolean = true,
    val showPhonetic: Boolean = true,
)

class PlayerViewModel(
    private val musicRepository: MusicRepository,
    private val controllerFuture: ListenableFuture<MediaController>,
) : ViewModel() {

    class Factory(private val context: Context) : ViewModelProvider.Factory {
        override fun <T : ViewModel> create(modelClass: Class<T>): T {
            if (modelClass.isAssignableFrom(PlayerViewModel::class.java)) {
                val repo = MusicRepositoryImpl(context.applicationContext)

                val sessionToken =
                    SessionToken(context, ComponentName(context, PlaybackService::class.java))
                val future = MediaController.Builder(context, sessionToken).buildAsync()

                @Suppress("UNCHECKED_CAST")
                return PlayerViewModel(repo, future) as T
            }
            throw IllegalArgumentException("Unknown ViewModel class")
        }
    }

    private val _uiState = MutableStateFlow(PlayerUiState())
    val uiState = _uiState.asStateFlow()
    private val _playbackState = MutableStateFlow(PlaybackSnapshot())
    val playbackState = _playbackState.asStateFlow()
    private var mediaController: MediaController? = null
    private var playbackDiscontinuity = 0L
    private var discontinuityPosition: Long? = null
    private var positionUpdateJob: Job? = null
    private var artworkClearJob: Job? = null
    private var artworkDecodeJob: Job? = null
    private var luminanceCalculationJob: Job? = null
    private var lastArtworkData: ByteArray? = null
    private val playerListener =
        object : Player.Listener {
            override fun onIsPlayingChanged(playing: Boolean) {
                if (playing) startPositionUpdates() else stopPositionUpdates()
            }

            override fun onPositionDiscontinuity(
                oldPosition: Player.PositionInfo,
                newPosition: Player.PositionInfo,
                reason: Int,
            ) {
                if (reason != Player.DISCONTINUITY_REASON_INTERNAL) {
                    playbackDiscontinuity++
                    discontinuityPosition = newPosition.positionMs
                }
            }

            override fun onEvents(player: Player, events: Player.Events) {
                if (events.contains(Player.EVENT_MEDIA_ITEM_TRANSITION)) playbackDiscontinuity++
                if (
                    events.containsAny(
                        Player.EVENT_MEDIA_METADATA_CHANGED,
                        Player.EVENT_MEDIA_ITEM_TRANSITION,
                        Player.EVENT_TIMELINE_CHANGED,
                        Player.EVENT_POSITION_DISCONTINUITY,
                        Player.EVENT_PLAYBACK_PARAMETERS_CHANGED,
                        Player.EVENT_IS_PLAYING_CHANGED,
                        Player.EVENT_PLAYBACK_STATE_CHANGED,
                    )
                ) {
                    updatePlaybackState(discontinuityPosition)
                    discontinuityPosition = null
                }
            }

            override fun onPlayerError(error: PlaybackException) {
                Log.e("PlayerViewModel", "Player Error: ${error.message}", error)
            }
        }

    init {
        viewModelScope.launch {
            try {
                val controller = controllerFuture.await()

                mediaController = controller
                controller.addListener(playerListener)

                updatePlaybackState()
                if (controller.isPlaying) {
                    startPositionUpdates()
                }
                val saved = musicRepository.lastSelection()
                if (saved != null)
                    onFilesSelected(saved.audio, saved.lyrics, saved.translation, autoPlay = false)
                else updateState { it.copy(showSelectionDialog = true) }
            } catch (e: Exception) {
                Log.e("PlayerViewModel", "Error connecting to MediaController", e)
                updateState { it.copy(showSelectionDialog = true, selectionError = e.message) }
            }
        }
    }

    private fun updateState(updater: (PlayerUiState) -> PlayerUiState) {
        _uiState.update(updater)
    }

    private var importJob: Job? = null

    fun onFilesSelected(
        audioUri: Uri,
        lyricsUri: Uri?,
        translationUri: Uri?,
        autoPlay: Boolean = true,
    ) {
        importJob?.cancel()
        importJob =
            viewModelScope.launch {
                updateState { it.copy(isImporting = true, selectionError = null) }
                try {
                    val item = musicRepository.createMusicItem(audioUri, lyricsUri, translationUri)
                    val lyrics = musicRepository.getLyricsFor(item)
                    val controller =
                        mediaController ?: error("Player is not ready yet. Please try again.")
                    if (
                        autoPlay || controller.currentMediaItem?.localConfiguration?.uri != audioUri
                    ) {
                        controller.setMediaItem(item.mediaItem)
                        controller.repeatMode = Player.REPEAT_MODE_ALL
                        controller.prepare()
                    }
                    if (autoPlay) controller.play()
                    musicRepository.saveSelection(
                        LocalFileSelection(item.uri, item.lyricsUri, item.translationUri)
                    )
                    updateState {
                        it.copy(
                            isImporting = false,
                            showSelectionDialog = false,
                            currentMusicItem = item,
                            lyrics = lyrics,
                        )
                    }
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (error: Exception) {
                    updateState {
                        it.copy(
                            isImporting = false,
                            selectionError = error.message ?: "Unable to open the selected files",
                            showSelectionDialog = true,
                        )
                    }
                }
            }
    }

    fun onDismissSongSelection() {
        importJob?.cancel()
        updateState {
            it.copy(showSelectionDialog = false, isImporting = false, selectionError = null)
        }
    }

    private fun updatePlaybackState(positionOverride: Long? = null) {
        val controller = mediaController ?: return
        val newArtworkData = controller.mediaMetadata.artworkData

        if (!newArtworkData.contentEquals(lastArtworkData)) {
            lastArtworkData = newArtworkData
            updateState { it.copy(artworkData = newArtworkData) }
            artworkClearJob?.cancel()
            artworkDecodeJob?.cancel()
            luminanceCalculationJob?.cancel()

            if (newArtworkData != null) {
                artworkDecodeJob =
                    viewModelScope.launch {
                        val bitmap =
                            withContext(Dispatchers.Default) {
                                decodePlayerArtwork(newArtworkData, IntSize(512, 512))
                            }
                        updateState { it.copy(backgroundState = BackgroundVisualState(bitmap, 0f)) }
                        if (bitmap != null) calculateAndApplyLuminance(bitmap)
                    }
            } else {
                artworkClearJob =
                    viewModelScope.launch {
                        delay(300)
                        updateState { it.copy(backgroundState = BackgroundVisualState(null, 0f)) }
                    }
            }
        }

        _playbackState.value =
            PlaybackSnapshot(
                isPlaying = controller.isPlaying,
                position = positionOverride ?: controller.currentPosition,
                duration = controller.duration.takeIf { it != C.TIME_UNSET } ?: 0L,
                sampledAtMillis = SystemClock.uptimeMillis(),
                speed = controller.playbackParameters.speed,
                discontinuity = playbackDiscontinuity,
            )
        updateState { it.copy(isReady = true) }
    }

    private fun calculateAndApplyLuminance(artwork: ImageBitmap) {
        luminanceCalculationJob?.cancel()
        luminanceCalculationJob =
            viewModelScope.launch(Dispatchers.Default) {
                try {
                    val bitmap = artwork.asAndroidBitmap()
                    val w = bitmap.width
                    val h = bitmap.height
                    val step = kotlin.math.max(1, kotlin.math.min(w, h) / 50)
                    var sum = 0.0
                    var count = 0
                    for (y in 0 until h step step) {
                        for (x in 0 until w step step) {
                            val c = bitmap.getPixel(x, y)
                            val r = (c shr 16) and 0xff
                            val g = (c shr 8) and 0xff
                            val b = c and 0xff
                            val lum = 0.2126 * r + 0.7152 * g + 0.0722 * b
                            sum += lum
                            count++
                        }
                    }
                    val avg = if (count > 0) (sum / count / 255.0).toFloat() else 0f
                    updateState {
                        it.copy(backgroundState = it.backgroundState.copy(luminance = avg))
                    }
                } catch (_: Exception) {
                    updateState {
                        it.copy(backgroundState = it.backgroundState.copy(luminance = 0f))
                    }
                }
            }
    }

    fun onOpenSongSelection() {
        updateState { it.copy(showSelectionDialog = true, selectionError = null) }
    }

    fun onShareRequested() {
        val controller = mediaController ?: return
        if (playbackState.value.isPlaying) {
            controller.pause()
        }
        updateState { it.copy(isShareSheetVisible = true) }
    }

    fun onShareDismissed() {
        val controller = mediaController ?: return
        if (!playbackState.value.isPlaying) {
            controller.play()
        }
        updateState { it.copy(isShareSheetVisible = false) }
    }

    private fun startPositionUpdates() {
        stopPositionUpdates()
        val controller = mediaController ?: return
        positionUpdateJob =
            viewModelScope.launch {
                while (isActive) {
                    _playbackState.update {
                        it.copy(
                            position = controller.currentPosition,
                            sampledAtMillis = SystemClock.uptimeMillis(),
                        )
                    }
                    delay(250)
                }
            }
    }

    private fun stopPositionUpdates() {
        positionUpdateJob?.cancel()
        positionUpdateJob = null
    }

    fun togglePlayPause() {
        val controller = mediaController ?: return
        if (controller.isPlaying) {
            controller.pause()
        } else {
            controller.play()
        }
    }

    internal fun recordPlaybackTiming(position: Int) {
        PlaybackTimingTrace.read(mediaController, position, _playbackState.value)
    }

    fun seekTo(position: Int) {
        val controller = mediaController ?: return
        val position = position.toLong()
        playbackDiscontinuity++
        _playbackState.update {
            it.copy(
                position = position,
                sampledAtMillis = SystemClock.uptimeMillis(),
                discontinuity = playbackDiscontinuity,
            )
        }
        controller.seekTo(position)
    }

    fun toggleTranslation() {
        updateState { it.copy(showTranslation = !it.showTranslation) }
    }

    fun togglePhonetic() {
        updateState { it.copy(showPhonetic = !it.showPhonetic) }
    }

    override fun onCleared() {
        super.onCleared()
        val controller = mediaController ?: return
        stopPositionUpdates()
        controller.removeListener(playerListener)
        controller.release()
        Log.d("PlayerViewModel", "ViewModel cleared and controller released.")
    }
}
