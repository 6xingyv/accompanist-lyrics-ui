package com.mocharealm.accompanist.sample.ui.theme

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.platform.Font
import com.mocharealm.accompanist.sample.Res
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import java.io.File
import java.net.URI
import java.nio.file.Files
import java.nio.file.StandardCopyOption

// Resource fonts copy this 22 MB file into both Java and Skia memory for each weight.
// File fonts let the native loader share its backing file across all variations.
private val sharedSFPro = CoroutineScope(Dispatchers.IO).async(start = CoroutineStart.LAZY) {
    val uri = URI(Res.getUri("font/sf_pro.ttf"))
    val file = if (uri.scheme == "file") File(uri) else {
        Files.createTempFile("accompanist-sf-pro-", ".ttf").toFile().apply {
            deleteOnExit()
            uri.toURL().openStream().use { Files.copy(it, toPath(), StandardCopyOption.REPLACE_EXISTING) }
        }
    }
    sfProFamily(file)
}

@OptIn(ExperimentalTextApi::class)
internal fun sfProFamily(file: File): FontFamily = FontFamily(
    listOf(FontWeight.ExtraLight, FontWeight.Light, FontWeight.Normal, FontWeight.Medium,
        FontWeight.SemiBold, FontWeight.Bold, FontWeight.ExtraBold).map { weight ->
        Font(file = file, weight = weight,
            variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight)))
    }
)

@Composable
actual fun SFPro(): FontFamily {
    val family by produceState<FontFamily>(FontFamily.Default) { value = sharedSFPro.await() }
    return family
}
