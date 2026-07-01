package com.mocharealm.accompanist.sample.data.repository

import android.content.ContentResolver
import android.content.ContentUris
import android.content.Context
import android.database.Cursor
import android.media.MediaMetadataRetriever
import android.net.Uri
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.MediaStore
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.core.parser.AutoParser
import com.mocharealm.accompanist.sample.domain.model.MusicItem
import com.mocharealm.accompanist.sample.domain.repository.MusicRepository
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.Charset
import java.nio.charset.CodingErrorAction
import java.util.Locale
import kotlin.math.min

class MusicRepositoryImpl(private val context: Context) : MusicRepository {
    private val autoParser = AutoParser()
    private val resolver: ContentResolver
        get() = context.contentResolver

    override suspend fun createMusicItem(
        audioUri: Uri,
        lyricsUri: Uri?,
        translationUri: Uri?
    ): MusicItem = withContext(Dispatchers.IO) {
        val audioPath = resolveFilePath(audioUri)
            ?: throw IllegalArgumentException("Unable to resolve selected audio path: $audioUri")
        val audioFile = File(audioPath)
        val audioMetadata = readAudioMetadata(audioFile)
        val fileNameMetadata = inferMetadataFromFileName(audioFile.nameWithoutExtension)
        val fallbackTitle = audioFile.nameWithoutExtension.ifBlank { audioFile.name }
        val title = audioMetadata.title ?: fileNameMetadata.title ?: fallbackTitle
        val artists = audioMetadata.artists ?: fileNameMetadata.artists ?: UNKNOWN_ARTIST

        val manualLyricsPath = lyricsUri?.let(::resolveFilePath)
        val manualTranslationPath = translationUri?.let(::resolveFilePath)

        MusicItem(
            label = title,
            artist = artists,
            titleFromAudioMetadata = audioMetadata.title != null,
            artistFromAudioMetadata = audioMetadata.artists != null,
            audioPath = audioFile.absolutePath,
            lyricsPath = manualLyricsPath ?: findExternalLyricsPath(audioFile),
            translationPath = manualTranslationPath,
            lyricsUri = lyricsUri.takeIf { manualLyricsPath == null },
            translationUri = translationUri.takeIf { manualTranslationPath == null },
            mediaItem = MediaItem.Builder()
                .setUri(Uri.fromFile(audioFile))
                .setMediaMetadata(
                    MediaMetadata.Builder()
                        .setTitle(title)
                        .setArtist(artists)
                        .build()
                )
                .build()
        )
    }

    override suspend fun findExternalLyricsPath(audioUri: Uri): String? = withContext(Dispatchers.IO) {
        resolveFilePath(audioUri)
            ?.let(::File)
            ?.let(::findExternalLyricsPath)
    }

    override suspend fun getLyricsFor(item: MusicItem): SyncedLyrics? = withContext(Dispatchers.IO) {
        val externalLyrics = readLyricsFromFileOrUri(item.lyricsPath, item.lyricsUri)

        val manualTranslationMap = readLyricsFromFileOrUri(item.translationPath, item.translationUri)
            ?.primaryTextByStart()

        if (externalLyrics != null) {
            manualTranslationMap?.let {
                return@withContext externalLyrics.withTranslations(it, replaceExisting = true)
            }
            if (externalLyrics.hasTranslation()) return@withContext externalLyrics

            val embeddedTranslationMap = readEmbeddedLyrics(File(item.audioPath))
                ?.translationByStart()
                .orEmpty()
            return@withContext if (embeddedTranslationMap.isNotEmpty()) {
                externalLyrics.withTranslations(embeddedTranslationMap)
            } else {
                externalLyrics
            }
        }

        val embeddedLyrics = readEmbeddedLyrics(File(item.audioPath)) ?: return@withContext null
        manualTranslationMap?.let {
            return@withContext embeddedLyrics.withTranslations(it, replaceExisting = true)
        }
        embeddedLyrics
    }

    private fun findExternalLyricsPath(audioFile: File): String? {
        val parent = audioFile.parentFile ?: return null
        val baseName = audioFile.nameWithoutExtension
        val siblings = parent.listFiles()?.filter { it.isFile } ?: return null
        return LYRIC_EXTENSIONS.firstNotNullOfOrNull { extension ->
            val expectedName = "$baseName.$extension"
            siblings.firstOrNull { it.name.equals(expectedName, ignoreCase = true) }?.absolutePath
        }
    }

    private data class AudioMetadata(
        val title: String?,
        val artists: String?
    )

    private data class FileNameMetadata(
        val title: String?,
        val artists: String?
    )

    private fun readAudioMetadata(audioFile: File): AudioMetadata {
        val retriever = MediaMetadataRetriever()
        return try {
            retriever.setDataSource(audioFile.absolutePath)
            AudioMetadata(
                title = retriever.extractCleanMetadata(MediaMetadataRetriever.METADATA_KEY_TITLE),
                artists = listOf(
                    MediaMetadataRetriever.METADATA_KEY_ARTIST,
                    MediaMetadataRetriever.METADATA_KEY_ALBUMARTIST,
                    MediaMetadataRetriever.METADATA_KEY_AUTHOR,
                    MediaMetadataRetriever.METADATA_KEY_COMPOSER
                ).firstNotNullOfOrNull { key -> retriever.extractCleanMetadata(key) }
            )
        } catch (_: Exception) {
            AudioMetadata(title = null, artists = null)
        } finally {
            runCatching { retriever.release() }
        }
    }

    private fun inferMetadataFromFileName(fileNameWithoutExtension: String): FileNameMetadata {
        val normalized = fileNameWithoutExtension.trim()
        val separators = listOf(" - ", " – ", " — ")
        val separator = separators.firstOrNull { normalized.contains(it) }
        if (separator != null) {
            val parts = normalized.split(separator, limit = 2)
            val artists = parts.getOrNull(0)?.cleanMetadataText()
            val title = parts.getOrNull(1)?.cleanMetadataText()
            if (title != null || artists != null) {
                return FileNameMetadata(title = title, artists = artists)
            }
        }

        return FileNameMetadata(title = normalized.cleanMetadataText(), artists = null)
    }

    private fun MediaMetadataRetriever.extractCleanMetadata(keyCode: Int): String? =
        extractMetadata(keyCode)?.cleanMetadataText()

    private fun String.cleanMetadataText(): String? {
        val value = trim { it == '\u0000' || it.isWhitespace() }
        if (value.isBlank()) return null
        val lower = value.lowercase(Locale.ROOT)
        return value.takeUnless { lower == "unknown" || lower == "<unknown>" || lower == "null" }
    }

    private fun readLyricsFromFileOrUri(path: String?, uri: Uri?): SyncedLyrics? {
        val content = path?.let { readText(File(it)) } ?: uri?.let(::readText)
        return content?.let(::parseLyrics)
    }

    private fun readText(file: File): String? =
        runCatching { file.readBytes().decodeLyricsText().cleanLyricsText().takeIf { it.isNotBlank() } }
            .getOrNull()

    private fun readText(uri: Uri): String? =
        runCatching {
            resolver.openInputStream(uri)?.use { it.readBytes() }
                ?.decodeLyricsText()
                ?.cleanLyricsText()
                ?.takeIf { it.isNotBlank() }
        }.getOrNull()

    private fun parseLyrics(content: String): SyncedLyrics? =
        runCatching { autoParser.parse(content).takeIf { it.lines.isNotEmpty() } }.getOrNull()

    private fun resolveFilePath(uri: Uri): String? {
        if (uri.scheme == ContentResolver.SCHEME_FILE) return uri.path
        if (uri.scheme != ContentResolver.SCHEME_CONTENT) return uri.path

        return resolveDocumentPath(uri)
            ?: queryDataColumn(uri)
    }

    private fun resolveDocumentPath(uri: Uri): String? =
        runCatching {
            if (!DocumentsContract.isDocumentUri(context, uri)) return null

            when (uri.authority) {
                EXTERNAL_STORAGE_DOCUMENTS_AUTHORITY -> resolveExternalStorageDocumentPath(uri)
                MEDIA_DOCUMENTS_AUTHORITY -> resolveMediaDocumentPath(uri)
                DOWNLOADS_DOCUMENTS_AUTHORITY -> resolveDownloadsDocumentPath(uri)
                else -> null
            }
        }.getOrNull()

    private fun resolveExternalStorageDocumentPath(uri: Uri): String? {
        val parts = DocumentsContract.getDocumentId(uri).split(':', limit = 2)
        if (parts.size != 2) return null

        val volume = parts[0]
        val relativePath = parts[1]
        return if (volume.equals("primary", ignoreCase = true)) {
            File(Environment.getExternalStorageDirectory(), relativePath).absolutePath
        } else {
            File("/storage/$volume", relativePath).absolutePath
        }
    }

    private fun resolveMediaDocumentPath(uri: Uri): String? {
        val parts = DocumentsContract.getDocumentId(uri).split(':', limit = 2)
        if (parts.size != 2) return null

        val id = parts[1]
        val collection = when (parts[0]) {
            "audio" -> MediaStore.Audio.Media.EXTERNAL_CONTENT_URI
            "video" -> MediaStore.Video.Media.EXTERNAL_CONTENT_URI
            "image" -> MediaStore.Images.Media.EXTERNAL_CONTENT_URI
            else -> MediaStore.Files.getContentUri("external")
        }

        return queryDataColumn(
            uri = collection,
            selection = "${MediaStore.MediaColumns._ID}=?",
            selectionArgs = arrayOf(id)
        )
    }

    private fun resolveDownloadsDocumentPath(uri: Uri): String? {
        val documentId = DocumentsContract.getDocumentId(uri)
        if (documentId.startsWith("raw:")) return documentId.removePrefix("raw:")

        val id = documentId.substringAfter(':', documentId).toLongOrNull() ?: return null
        val downloadsUri = ContentUris.withAppendedId(
            Uri.parse("content://downloads/public_downloads"),
            id
        )
        return queryDataColumn(downloadsUri)
    }

    private fun queryDataColumn(
        uri: Uri,
        selection: String? = null,
        selectionArgs: Array<String>? = null
    ): String? =
        runCatching {
            resolver.query(
                uri,
                arrayOf(MediaStore.MediaColumns.DATA),
                selection,
                selectionArgs,
                null
            )?.use { cursor ->
                if (cursor.moveToFirst()) cursor.stringOrNull(MediaStore.MediaColumns.DATA) else null
            }
        }.getOrNull()

    private fun readEmbeddedLyrics(audioFile: File): SyncedLyrics? {
        val bytes = runCatching { audioFile.readBytes() }.getOrNull() ?: return null
        val embeddedTexts = buildList {
            addAll(extractId3Lyrics(bytes))
            addAll(extractMp4Lyrics(bytes))
            addAll(extractVorbisLyrics(bytes))
        }

        return embeddedTexts.asSequence()
            .mapNotNull { parseLyrics(it) }
            .firstOrNull { it.lines.isNotEmpty() }
    }

    private fun extractId3Lyrics(data: ByteArray): List<String> {
        if (data.size < ID3_HEADER_SIZE) return emptyList()
        if (data[0] != 'I'.code.toByte() || data[1] != 'D'.code.toByte() || data[2] != '3'.code.toByte()) {
            return emptyList()
        }

        val majorVersion = data[3].toInt() and 0xff
        if (majorVersion !in 2..4) return emptyList()

        val flags = data[5].toInt() and 0xff
        val tagUnsynchronized = (flags and 0x80) != 0
        val tagEnd = min(data.size, ID3_HEADER_SIZE + readSyncSafeInt(data, 6))
        var cursor = ID3_HEADER_SIZE

        if ((flags and 0x40) != 0 && cursor + 4 <= tagEnd) {
            val extendedHeaderSize = if (majorVersion == 4) {
                readSyncSafeInt(data, cursor)
            } else {
                readInt32(data, cursor)
            }
            cursor += if (majorVersion == 3) 4 + extendedHeaderSize else extendedHeaderSize
        }

        val result = mutableListOf<String>()
        val frameIdLength = if (majorVersion == 2) 3 else 4
        val frameHeaderSize = if (majorVersion == 2) 6 else 10

        while (cursor + frameHeaderSize <= tagEnd) {
            val frameId = data.decodeToLatin1(cursor, cursor + frameIdLength)
            if (frameId.all { it == '\u0000' }) break

            val frameSize = when (majorVersion) {
                2 -> readUInt24(data, cursor + 3)
                4 -> readSyncSafeInt(data, cursor + 4)
                else -> readInt32(data, cursor + 4)
            }
            if (frameSize <= 0) break

            val payloadStart = cursor + frameHeaderSize
            val payloadEnd = payloadStart + frameSize
            if (payloadEnd > tagEnd || payloadEnd > data.size) break

            val frameUnsynchronized =
                majorVersion == 4 && frameHeaderSize == 10 && ((data[cursor + 9].toInt() and 0x02) != 0)
            val payload = data.copyOfRange(payloadStart, payloadEnd)
                .let { if (tagUnsynchronized || frameUnsynchronized) it.removeId3Unsynchronization() else it }

            when (frameId) {
                "USLT", "ULT" -> parseId3UnsyncedLyricsFrame(payload)?.let(result::add)
                "SYLT", "SLT" -> parseId3SyncedLyricsFrame(payload)?.let(result::add)
                "TXXX", "TXX" -> parseId3UserTextLyricsFrame(payload)?.let(result::add)
                "COMM", "COM" -> parseId3CommentLyricsFrame(payload)?.let(result::add)
            }

            cursor = payloadEnd
        }

        return result.distinct()
    }

    private fun parseId3UnsyncedLyricsFrame(payload: ByteArray): String? {
        if (payload.size < 5) return null
        val encoding = payload[0].toInt() and 0xff
        val descriptionEnd = findId3TextTerminator(payload, 4, encoding)
        val textStart = (descriptionEnd + id3TerminatorLength(encoding)).coerceAtMost(payload.size)
        return decodeId3Text(payload, textStart, payload.size, encoding).cleanLyricsText()
            .takeIf { it.isNotBlank() }
    }

    private fun parseId3SyncedLyricsFrame(payload: ByteArray): String? {
        if (payload.size < 7) return null
        val encoding = payload[0].toInt() and 0xff
        val timestampFormat = payload[4].toInt() and 0xff
        if (timestampFormat != 2) return null

        val descriptionEnd = findId3TextTerminator(payload, 6, encoding)
        var cursor = (descriptionEnd + id3TerminatorLength(encoding)).coerceAtMost(payload.size)
        val lines = mutableListOf<String>()

        while (cursor < payload.size) {
            val textEnd = findId3TextTerminator(payload, cursor, encoding)
            if (textEnd >= payload.size) break
            val text = decodeId3Text(payload, cursor, textEnd, encoding).cleanLyricsText()
            cursor = textEnd + id3TerminatorLength(encoding)
            if (cursor + 4 > payload.size) break

            val time = readInt32(payload, cursor)
            cursor += 4
            if (text.isNotBlank()) lines += "${time.toLrcTime()}$text"
        }

        return lines.joinToString("\n").takeIf { it.isNotBlank() }
    }

    private fun parseId3UserTextLyricsFrame(payload: ByteArray): String? {
        if (payload.isEmpty()) return null
        val encoding = payload[0].toInt() and 0xff
        val descriptionEnd = findId3TextTerminator(payload, 1, encoding)
        val description = decodeId3Text(payload, 1, descriptionEnd, encoding)
        val valueStart = (descriptionEnd + id3TerminatorLength(encoding)).coerceAtMost(payload.size)
        val value = decodeId3Text(payload, valueStart, payload.size, encoding).cleanLyricsText()

        return value.takeIf {
            it.isNotBlank() && (description.contains("lyric", ignoreCase = true) || it.looksLikeTimedLyrics())
        }
    }

    private fun parseId3CommentLyricsFrame(payload: ByteArray): String? {
        if (payload.size < 5) return null
        val encoding = payload[0].toInt() and 0xff
        val descriptionEnd = findId3TextTerminator(payload, 4, encoding)
        val description = decodeId3Text(payload, 4, descriptionEnd, encoding)
        val textStart = (descriptionEnd + id3TerminatorLength(encoding)).coerceAtMost(payload.size)
        val text = decodeId3Text(payload, textStart, payload.size, encoding).cleanLyricsText()

        return text.takeIf {
            it.isNotBlank() && (description.contains("lyric", ignoreCase = true) || it.looksLikeTimedLyrics())
        }
    }

    private fun extractMp4Lyrics(data: ByteArray): List<String> {
        val result = mutableListOf<String>()
        var typeOffset = 4
        while (typeOffset + 4 <= data.size) {
            if ((data[typeOffset].toInt() and 0xff) == 0xa9 &&
                data[typeOffset + 1] == 'l'.code.toByte() &&
                data[typeOffset + 2] == 'y'.code.toByte() &&
                data[typeOffset + 3] == 'r'.code.toByte()
            ) {
                val atomStart = typeOffset - 4
                val atomSize = readUInt32(data, atomStart).toInt()
                parseMp4TextAtom(data, atomStart, atomSize)?.let(result::add)
            }
            typeOffset++
        }
        return result.distinct()
    }

    private fun parseMp4TextAtom(data: ByteArray, atomStart: Int, atomSize: Int): String? {
        if (atomSize < 16 || atomStart + atomSize > data.size) return null
        val atomEnd = atomStart + atomSize
        var cursor = atomStart + 8

        while (cursor + 8 <= atomEnd) {
            val childSize = readUInt32(data, cursor).toInt()
            val childType = data.decodeToLatin1(cursor + 4, cursor + 8)
            if (childSize < 8 || cursor + childSize > atomEnd) break

            if (childType == "data" && childSize >= 16) {
                return data.copyOfRange(cursor + 16, cursor + childSize)
                    .decodeLyricsText()
                    .cleanLyricsText()
                    .takeIf { it.isNotBlank() }
            }

            cursor += childSize
        }

        return null
    }

    private fun extractVorbisLyrics(data: ByteArray): List<String> {
        val result = mutableListOf<String>()

        if (data.size > 4 && data.decodeToLatin1(0, 4) == "fLaC") {
            var cursor = 4
            var lastBlock = false
            while (!lastBlock && cursor + 4 <= data.size) {
                val header = data[cursor].toInt() and 0xff
                lastBlock = (header and 0x80) != 0
                val blockType = header and 0x7f
                val blockSize = readUInt24(data, cursor + 1)
                val payloadStart = cursor + 4
                val payloadEnd = payloadStart + blockSize
                if (payloadEnd > data.size) break
                if (blockType == 4) {
                    result += parseVorbisCommentPayload(data, payloadStart, payloadEnd)
                }
                cursor = payloadEnd
            }
        }

        indexOfBytes(
            data,
            byteArrayOf(
                3,
                'v'.code.toByte(),
                'o'.code.toByte(),
                'r'.code.toByte(),
                'b'.code.toByte(),
                'i'.code.toByte(),
                's'.code.toByte()
            )
        )?.let { result += parseVorbisCommentPayload(data, it + 7, data.size) }

        indexOfBytes(data, "OpusTags".toByteArray(Charsets.ISO_8859_1))
            ?.let { result += parseVorbisCommentPayload(data, it + 8, data.size) }

        return result.distinct()
    }

    private fun parseVorbisCommentPayload(data: ByteArray, offset: Int, limit: Int): List<String> {
        val result = mutableListOf<String>()
        var cursor = offset
        if (cursor + 8 > limit) return emptyList()

        val vendorLength = readLittleEndianInt(data, cursor)
        cursor += 4 + vendorLength
        if (vendorLength < 0 || cursor + 4 > limit) return emptyList()

        val commentCount = readLittleEndianInt(data, cursor)
        cursor += 4
        if (commentCount !in 0..10_000) return emptyList()

        repeat(commentCount) {
            if (cursor + 4 > limit) return@repeat
            val length = readLittleEndianInt(data, cursor)
            cursor += 4
            if (length < 0 || cursor + length > limit) return@repeat

            val comment = data.copyOfRange(cursor, cursor + length).decodeLyricsText()
            cursor += length
            val separator = comment.indexOf('=')
            if (separator <= 0) return@repeat

            val key = comment.substring(0, separator).uppercase(Locale.ROOT)
            val value = comment.substring(separator + 1).cleanLyricsText()
            if (key in VORBIS_LYRIC_KEYS && value.isNotBlank()) {
                result += value
            }
        }

        return result
    }

    private fun SyncedLyrics.hasTranslation(): Boolean =
        lines.any { it.hasTranslation() }

    private fun ISyncedLine.hasTranslation(): Boolean = when (this) {
        is KaraokeLine.MainKaraokeLine ->
            translation.hasText() || accompanimentLines.orEmpty().any { it.translation.hasText() }

        is KaraokeLine -> translation.hasText()
        is SyncedLine -> translation.hasText()
        else -> false
    }

    private fun SyncedLyrics.translationByStart(): Map<Int, String> {
        val translations = mutableMapOf<Int, String>()
        lines.forEach { line ->
            when (line) {
                is KaraokeLine.MainKaraokeLine -> {
                    line.translation?.takeIf { it.isNotBlank() }?.let { translations.putIfAbsent(line.start, it) }
                    line.accompanimentLines.orEmpty().forEach { accompaniment ->
                        accompaniment.translation?.takeIf { it.isNotBlank() }
                            ?.let { translations.putIfAbsent(accompaniment.start, it) }
                    }
                }

                is KaraokeLine -> line.translation?.takeIf { it.isNotBlank() }
                    ?.let { translations.putIfAbsent(line.start, it) }

                is SyncedLine -> line.translation?.takeIf { it.isNotBlank() }
                    ?.let { translations.putIfAbsent(line.start, it) }
            }
        }
        return translations
    }

    private fun SyncedLyrics.primaryTextByStart(): Map<Int, String> {
        val translations = mutableMapOf<Int, String>()
        lines.forEach { line ->
            val text = line.primaryText()?.takeIf { it.isNotBlank() } ?: return@forEach
            translations.putIfAbsent(line.start, text)
        }
        return translations
    }

    private fun ISyncedLine.primaryText(): String? = when (this) {
        is KaraokeLine -> syllables.joinToString("") { it.content }.trim()
        is SyncedLine -> content.trim()
        else -> null
    }

    private fun SyncedLyrics.withTranslations(
        translations: Map<Int, String>,
        replaceExisting: Boolean = false
    ): SyncedLyrics {
        if (translations.isEmpty()) return this

        return copy(
            lines = lines.map { line ->
                when (line) {
                    is KaraokeLine.MainKaraokeLine -> line.copy(
                        translation = line.translation.mergedWith(translations[line.start], replaceExisting),
                        accompanimentLines = line.accompanimentLines?.map { accompaniment ->
                            accompaniment.copy(
                                translation = accompaniment.translation
                                    .mergedWith(translations[accompaniment.start], replaceExisting)
                            )
                        }
                    )

                    is KaraokeLine.AccompanimentKaraokeLine -> line.copy(
                        translation = line.translation.mergedWith(translations[line.start], replaceExisting)
                    )

                    is SyncedLine -> line.copy(
                        translation = line.translation.mergedWith(translations[line.start], replaceExisting)
                    )

                    else -> line
                }
            }
        )
    }

    private fun String?.mergedWith(candidate: String?, replaceExisting: Boolean): String? =
        if (replaceExisting || !this.hasText()) candidate ?: this else this

    private fun String?.hasText(): Boolean = !isNullOrBlank()

    private fun ByteArray.decodeLyricsText(): String {
        if (size >= 3 && this[0] == 0xef.toByte() && this[1] == 0xbb.toByte() && this[2] == 0xbf.toByte()) {
            return copyOfRange(3, size).toString(Charsets.UTF_8)
        }
        if (size >= 2 && this[0] == 0xff.toByte() && this[1] == 0xfe.toByte()) {
            return copyOfRange(2, size).toString(Charsets.UTF_16LE)
        }
        if (size >= 2 && this[0] == 0xfe.toByte() && this[1] == 0xff.toByte()) {
            return copyOfRange(2, size).toString(Charsets.UTF_16BE)
        }

        val charsets = buildList {
            add(Charsets.UTF_8)
            runCatching { Charset.forName("GB18030") }.getOrNull()?.let(::add)
            add(Charsets.UTF_16LE)
            add(Charsets.UTF_16BE)
            add(Charsets.ISO_8859_1)
        }

        for (charset in charsets) {
            try {
                return charset.newDecoder()
                    .onMalformedInput(CodingErrorAction.REPORT)
                    .onUnmappableCharacter(CodingErrorAction.REPORT)
                    .decode(ByteBuffer.wrap(this))
                    .toString()
            } catch (_: CharacterCodingException) {
                continue
            }
        }

        return toString(Charsets.UTF_8)
    }

    private fun String.cleanLyricsText(): String =
        trim { it == '\u0000' || it == '\ufeff' || it.isWhitespace() }

    private fun String.looksLikeTimedLyrics(): Boolean {
        val lower = lowercase(Locale.ROOT)
        return lower.contains("<tt") ||
            lower.contains("[language:") ||
            LRC_TIME_REGEX.containsMatchIn(this) ||
            KRC_TIME_REGEX.containsMatchIn(this)
    }

    private fun decodeId3Text(data: ByteArray, start: Int, end: Int, encoding: Int): String {
        if (start >= end || start >= data.size) return ""
        val safeEnd = end.coerceAtMost(data.size)
        val bytes = data.copyOfRange(start, safeEnd)
        val charset = when (encoding) {
            0 -> Charsets.ISO_8859_1
            1 -> Charsets.UTF_16
            2 -> Charsets.UTF_16BE
            3 -> Charsets.UTF_8
            else -> Charsets.UTF_8
        }
        return bytes.toString(charset)
    }

    private fun findId3TextTerminator(data: ByteArray, start: Int, encoding: Int): Int {
        val terminatorLength = id3TerminatorLength(encoding)
        if (terminatorLength == 1) {
            for (index in start until data.size) {
                if (data[index] == 0.toByte()) return index
            }
            return data.size
        }

        var index = start
        while (index + 1 < data.size) {
            if (data[index] == 0.toByte() && data[index + 1] == 0.toByte()) return index
            index += 2
        }
        return data.size
    }

    private fun id3TerminatorLength(encoding: Int): Int =
        if (encoding == 1 || encoding == 2) 2 else 1

    private fun ByteArray.removeId3Unsynchronization(): ByteArray {
        val output = ArrayList<Byte>(size)
        var index = 0
        while (index < size) {
            val byte = this[index]
            output += byte
            if ((byte.toInt() and 0xff) == 0xff && index + 1 < size && this[index + 1] == 0.toByte()) {
                index += 2
            } else {
                index++
            }
        }
        return output.toByteArray()
    }

    private fun Int.toLrcTime(): String {
        val minutes = this / 60_000
        val seconds = (this % 60_000) / 1_000
        val hundredths = (this % 1_000) / 10
        return "[%02d:%02d.%02d]".format(Locale.ROOT, minutes, seconds, hundredths)
    }

    private fun Cursor.stringOrNull(columnName: String): String? {
        val index = getColumnIndex(columnName)
        return if (index >= 0 && !isNull(index)) getString(index) else null
    }

    private fun ByteArray.decodeToLatin1(start: Int, end: Int): String =
        copyOfRange(start, end.coerceAtMost(size)).toString(Charsets.ISO_8859_1)

    private fun readSyncSafeInt(data: ByteArray, offset: Int): Int =
        ((data[offset].toInt() and 0x7f) shl 21) or
            ((data[offset + 1].toInt() and 0x7f) shl 14) or
            ((data[offset + 2].toInt() and 0x7f) shl 7) or
            (data[offset + 3].toInt() and 0x7f)

    private fun readInt32(data: ByteArray, offset: Int): Int =
        ((data[offset].toInt() and 0xff) shl 24) or
            ((data[offset + 1].toInt() and 0xff) shl 16) or
            ((data[offset + 2].toInt() and 0xff) shl 8) or
            (data[offset + 3].toInt() and 0xff)

    private fun readUInt32(data: ByteArray, offset: Int): Long =
        ((data[offset].toLong() and 0xff) shl 24) or
            ((data[offset + 1].toLong() and 0xff) shl 16) or
            ((data[offset + 2].toLong() and 0xff) shl 8) or
            (data[offset + 3].toLong() and 0xff)

    private fun readUInt24(data: ByteArray, offset: Int): Int =
        ((data[offset].toInt() and 0xff) shl 16) or
            ((data[offset + 1].toInt() and 0xff) shl 8) or
            (data[offset + 2].toInt() and 0xff)

    private fun readLittleEndianInt(data: ByteArray, offset: Int): Int =
        (data[offset].toInt() and 0xff) or
            ((data[offset + 1].toInt() and 0xff) shl 8) or
            ((data[offset + 2].toInt() and 0xff) shl 16) or
            ((data[offset + 3].toInt() and 0xff) shl 24)

    private fun indexOfBytes(data: ByteArray, target: ByteArray): Int? {
        if (target.isEmpty() || target.size > data.size) return null
        for (index in 0..data.size - target.size) {
            var matched = true
            for (targetIndex in target.indices) {
                if (data[index + targetIndex] != target[targetIndex]) {
                    matched = false
                    break
                }
            }
            if (matched) return index
        }
        return null
    }

    companion object {
        private val LYRIC_EXTENSIONS = listOf("ttml", "lrc", "elrc", "krc")
        private val VORBIS_LYRIC_KEYS = setOf("LYRICS", "UNSYNCEDLYRICS", "SYNCEDLYRICS")
        private val LRC_TIME_REGEX = Regex("""\[\d{1,3}:\d{2}[.:]\d{1,3}]""")
        private val KRC_TIME_REGEX = Regex("""(?m)^\[\d+,\d+]""")
        private const val UNKNOWN_ARTIST = "Unknown Artist"
        private const val EXTERNAL_STORAGE_DOCUMENTS_AUTHORITY =
            "com.android.externalstorage.documents"
        private const val MEDIA_DOCUMENTS_AUTHORITY = "com.android.providers.media.documents"
        private const val DOWNLOADS_DOCUMENTS_AUTHORITY = "com.android.providers.downloads.documents"
        private const val ID3_HEADER_SIZE = 10
    }
}
