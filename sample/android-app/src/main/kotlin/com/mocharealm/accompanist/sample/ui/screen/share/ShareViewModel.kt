package com.mocharealm.accompanist.sample.ui.screen.share

import android.content.ContentValues
import android.content.Context
import android.graphics.Bitmap
import android.net.Uri
import android.provider.MediaStore
import androidx.core.content.FileProvider
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.sample.ui.composable.background.BackgroundVisualState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.io.File
import java.io.FileOutputStream

enum class ShareStep {
    SELECTING, GENERATING
}

data class ShareContext(
    val lyrics: SyncedLyrics,
    val initialLine: KaraokeLine,
    val backgroundState: BackgroundVisualState,
    val title: String,
    val artist: String,
    val cover: Bitmap
)

data class ShareUiState(
    val context: ShareContext? = null,
    val step: ShareStep = ShareStep.SELECTING,
    val selectedLines: List<KaraokeLine> = emptyList()
)

class ShareViewModel : ViewModel() {
    private val _uiState = MutableStateFlow(ShareUiState())
    val uiState = _uiState.asStateFlow()

    private val _toastEvent = MutableSharedFlow<String>()
    val toastEvent = _toastEvent.asSharedFlow()

    private val _shareEvent = MutableSharedFlow<Uri>()
    val shareEvent = _shareEvent.asSharedFlow()


    fun prepareForSharing(context: ShareContext) {
        _uiState.update {
            it.copy(
                context = context,
                step = ShareStep.SELECTING,
                selectedLines = listOf(context.initialLine) // Start with the pressed line selected
            )
        }
    }

    fun toggleLineSelection(line: KaraokeLine) {
        val currentSelected = _uiState.value.selectedLines
        val newSelected = if (currentSelected.contains(line)) {
            if (currentSelected.size>1) {
                currentSelected - line
            } else currentSelected
        } else {
            if (currentSelected.size < 5) (currentSelected + line).sortedBy {
                _uiState.value.context?.lyrics?.lines?.indexOf(
                    it
                )
            }
            else currentSelected
        }
        _uiState.update { it.copy(selectedLines = newSelected) }
    }

    fun proceedToGenerate() {
        if (_uiState.value.selectedLines.isNotEmpty()) {
            _uiState.update { it.copy(step = ShareStep.GENERATING) }
        }
    }

    fun returnToSelection() {
        _uiState.update { it.copy(step = ShareStep.SELECTING) }
    }

    fun reset() {
        _uiState.value = ShareUiState()
    }

    fun saveBitmapToGallery(context: Context, bitmap: Bitmap) {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val contentValues = ContentValues().apply {
                    put(MediaStore.MediaColumns.DISPLAY_NAME, "Accompanist_Share_${System.currentTimeMillis()}.png")
                    put(MediaStore.MediaColumns.MIME_TYPE, "image/png")
                    put(MediaStore.MediaColumns.RELATIVE_PATH, "Pictures/Accompanist")
                }

                val contentResolver = context.contentResolver
                val uri = contentResolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, contentValues)

                uri?.let {
                    contentResolver.openOutputStream(it)?.use { outputStream ->
                        bitmap.compress(Bitmap.CompressFormat.PNG, 100, outputStream)
                        _toastEvent.emit("Saved to Gallery")
                    } ?: throw Exception("Failed to open output stream.")
                } ?: throw Exception("MediaStore insert failed.")

            } catch (e: Exception) {
                _toastEvent.emit("Save failed: ${e.message}")
            }
        }
    }

    fun prepareForSharing(context: Context, bitmap: Bitmap) {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                // 1. Save Bitmap to app's cache directory
                val cachePath = File(context.cacheDir, "shared_images")
                cachePath.mkdirs() // Create directory
                val imageFile = File(cachePath, "share_${System.currentTimeMillis()}.png")
                val fos = FileOutputStream(imageFile)
                bitmap.compress(Bitmap.CompressFormat.PNG, 100, fos)
                fos.close()

                // 2. Use FileProvider to get a secure content:// URI
                val uri = FileProvider.getUriForFile(
                    context,
                    "${context.packageName}.fileprovider", // Your FileProvider authority
                    imageFile
                )

                // 3. Emit share event, UI layer will listen to this URI and start sharing
                _shareEvent.emit(uri)

            } catch (e: Exception) {
                _toastEvent.emit("Share failed: ${e.message}")
            }
        }
    }

}