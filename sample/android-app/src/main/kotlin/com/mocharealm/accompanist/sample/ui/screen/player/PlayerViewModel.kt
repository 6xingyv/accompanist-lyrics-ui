package com.mocharealm.accompanist.sample.ui.screen.player

import android.content.ComponentName
import android.content.Context
import android.graphics.BitmapFactory
import android.util.Log
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.media3.common.C
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.ListenableFuture
import com.mocharealm.accompanist.sample.data.repository.MusicRepositoryImpl
import com.mocharealm.accompanist.sample.domain.model.MusicItem
import com.mocharealm.accompanist.sample.domain.repository.MusicRepository
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.sample.ui.composable.background.BackgroundVisualState
import com.mocharealm.accompanist.sample.service.PlaybackService
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.guava.await
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

data class PlaybackState(
    val isPlaying: Boolean = false,
    val position: Long = 0L,
    val duration: Long = 0L,
    val lastUpdateTime: Long = 0L
)

data class PlayerUiState(
    val isReady: Boolean = false,
    val showSelectionDialog: Boolean = true,
    val playbackState: PlaybackState = PlaybackState(),
    val backgroundState: BackgroundVisualState = BackgroundVisualState(null, 0f),
    val lyrics: SyncedLyrics? = null,
    val availableSongs: List<MusicItem> = emptyList(),
    val currentMusicItem: MusicItem? = null,
    val isShareSheetVisible: Boolean = false
)

class PlayerViewModel(
    private val musicRepository: MusicRepository,
    private val controllerFuture: ListenableFuture<MediaController>
) : ViewModel() {

    class Factory(private val context: Context) : ViewModelProvider.Factory {
        override fun <T : ViewModel> create(modelClass: Class<T>): T {
            if (modelClass.isAssignableFrom(PlayerViewModel::class.java)) {
                val repo = MusicRepositoryImpl(context.applicationContext)

                val sessionToken = SessionToken(context, ComponentName(context, PlaybackService::class.java))
                val future = MediaController.Builder(context, sessionToken).buildAsync()

                @Suppress("UNCHECKED_CAST")
                return PlayerViewModel(repo, future) as T
            }
            throw IllegalArgumentException("Unknown ViewModel class")
        }
    }

    private val _uiState = MutableStateFlow(PlayerUiState())
    val uiState = _uiState.asStateFlow()
    private var mediaController: MediaController? = null
    private var positionUpdateJob: Job? = null
    private var artworkClearJob: Job? = null
    private var luminanceCalculationJob: Job? = null
    private var lastArtworkData: ByteArray? = null
    private val playerListener = object : Player.Listener {
        override fun onIsPlayingChanged(playing: Boolean) {
            updatePlaybackState()
            if (playing) startPositionUpdates() else stopPositionUpdates()
        }

        override fun onEvents(player: Player, events: Player.Events) {
            if (events.containsAny(Player.EVENT_MEDIA_METADATA_CHANGED, Player.EVENT_TIMELINE_CHANGED)) {
                updatePlaybackState()
            }
        }

        override fun onPlayerError(error: PlaybackException) {
            Log.e("PlayerViewModel", "Player Error: ${error.message}", error)
        }
    }

    init {
        loadAvailableSongs()
        viewModelScope.launch {
            try {
                val controller = controllerFuture.await()

                mediaController = controller
                controller.addListener(playerListener)

                updatePlaybackState()
                if (controller.isPlaying) {
                    startPositionUpdates()
                }

            } catch (e: Exception) {
                Log.e("PlayerViewModel", "Error connecting to MediaController", e)
            }
        }
    }

    private fun updateState(updater: (PlayerUiState) -> PlayerUiState) {
        _uiState.update(updater)
    }

    private fun loadAvailableSongs() {
        viewModelScope.launch {
            val songs =  musicRepository.getMusicItems()
            updateState { it.copy(availableSongs = songs) }
        }
    }

    private fun loadLyricsFor(item: MusicItem) {
        viewModelScope.launch {
            val lyrics = musicRepository.getLyricsFor(item)
            updateState { it.copy(lyrics = lyrics) }
        }
    }

    private fun updatePlaybackState() {
        val controller = mediaController ?: return
        val newArtworkData = controller.mediaMetadata.artworkData

        if (!newArtworkData.contentEquals(lastArtworkData)) {
            lastArtworkData = newArtworkData
            artworkClearJob?.cancel()

            if (newArtworkData != null) {
                val newArtworkBitmap = BitmapFactory.decodeByteArray(newArtworkData, 0, newArtworkData.size).asImageBitmap()

                updateState { it.copy(backgroundState = it.backgroundState.copy(newArtworkBitmap)) }
                calculateAndApplyLuminance(newArtworkBitmap)
            } else {
                artworkClearJob = viewModelScope.launch {
                    delay(300)
                    updateState { it.copy(backgroundState = BackgroundVisualState(null, 0f)) }
                }
            }
        }

        updateState { currentState ->
            currentState.copy(
                playbackState = currentState.playbackState.copy(
                    isPlaying = controller.isPlaying,
                    position = controller.currentPosition,
                    duration = controller.duration.takeIf { it != C.TIME_UNSET } ?: 0L,
                    lastUpdateTime = System.currentTimeMillis()
                ),
                isReady = true
            )
        }
    }

    private fun calculateAndApplyLuminance(artwork: ImageBitmap) {
        luminanceCalculationJob?.cancel()
        luminanceCalculationJob = viewModelScope.launch(Dispatchers.Default) {
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
                updateState { it.copy(backgroundState = it.backgroundState.copy(luminance = avg)) }
            } catch (_: Exception) {
                updateState { it.copy(backgroundState = it.backgroundState.copy(luminance = 0f)) }
            }
        }
    }

    fun onSongSelected(item: MusicItem) {
        val controller = mediaController ?: return
        updateState { it.copy(showSelectionDialog = false, currentMusicItem = item) }
        controller.setMediaItem(item.mediaItem)
        controller.repeatMode = Player.REPEAT_MODE_ALL
        controller.prepare()
        controller.play()
        loadLyricsFor(item)
    }

    fun onOpenSongSelection() {
        val controller = mediaController ?: return
        controller.stop()
        updateState { it.copy(showSelectionDialog = true, lyrics = null, currentMusicItem = null) }
    }

    fun onShareRequested() {
        val controller = mediaController ?: return
        if (uiState.value.playbackState.isPlaying) {
            controller.pause()
        }
        updateState { it.copy(isShareSheetVisible = true) }
    }

    fun onShareDismissed() {
        val controller = mediaController ?: return
        if (!uiState.value.playbackState.isPlaying) {
            controller.play()
        }
        updateState { it.copy(isShareSheetVisible = false) }
    }

    private fun startPositionUpdates() {
        stopPositionUpdates()
        val controller = mediaController ?: return
        positionUpdateJob = viewModelScope.launch {
            while (isActive) {
                updateState { currentState ->
                    currentState.copy(
                        playbackState = currentState.playbackState.copy(
                            position = controller.currentPosition,
                            lastUpdateTime = System.currentTimeMillis()
                        )
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

    fun seekTo(position: Int) {
        val controller = mediaController ?: return
        val position = position.toLong()
        controller.seekTo(position)
        updateState { currentState ->
            currentState.copy(
                playbackState = currentState.playbackState.copy(
                    position = position,
                    lastUpdateTime = System.currentTimeMillis()
                )
            )
        }
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