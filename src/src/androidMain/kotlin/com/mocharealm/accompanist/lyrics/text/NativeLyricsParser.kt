package com.mocharealm.accompanist.lyrics.text

import com.mocharealm.accompanist.lyrics.core.model.Artist
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.charset.StandardCharsets

object NativeLyricsParser {
    init {
        System.loadLibrary("text_engine")
    }

    fun parse(content: String): SyncedLyrics? = parse(content, null)

    fun parse(content: String, onPlainText: ((ByteBuffer) -> Unit)?): SyncedLyrics? = runCatching {
        withNativeBuffer(nativeParseToWire(content)) { decodeLyrics(it, onPlainText) }
    }.getOrNull()

    fun parseFd(fd: Int): SyncedLyrics? = parseFd(fd, null)

    fun parseFd(fd: Int, onPlainText: ((ByteBuffer) -> Unit)?): SyncedLyrics? = runCatching {
        withNativeBuffer(nativeParseFdToWire(fd)) { decodeLyrics(it, onPlainText) }
    }.getOrNull()

    fun <T> withPlainTextBuffer(content: String, block: (ByteBuffer) -> T): T? =
        withNativeBuffer(nativeParseToPlainText(content), block)

    fun <T> withPlainTextBufferFd(fd: Int, block: (ByteBuffer) -> T): T? =
        withNativeBuffer(nativeParseFdToPlainText(fd), block)

    private fun <T> withNativeBuffer(buffer: ByteBuffer?, block: (ByteBuffer) -> T): T? {
        if (buffer == null) return null
        return try {
            buffer.order(ByteOrder.LITTLE_ENDIAN)
            block(buffer)
        } finally {
            nativeReleaseBuffer(buffer)
        }
    }

    private fun decodeLyrics(
        buffer: ByteBuffer,
        onPlainText: ((ByteBuffer) -> Unit)?,
    ): SyncedLyrics? {
        val magic = ByteArray(4)
        buffer.get(magic)
        if (!magic.contentEquals(byteArrayOf('L'.code.toByte(), 'Y'.code.toByte(), 'R'.code.toByte(), '1'.code.toByte()))) {
            return null
        }
        val title = buffer.readString()
        val id = buffer.readString()
        val artists = List(buffer.readCount()) {
            Artist(type = buffer.readString(), name = buffer.readString())
        }
        val lines = List(buffer.readCount()) { buffer.readLine() }
        if (buffer.remaining() >= Int.SIZE_BYTES) {
            val plainTextLength = buffer.readCount()
            require(plainTextLength <= buffer.remaining()) { "Truncated native plain-text lyrics" }
            if (plainTextLength > 0) {
                val plainText = buffer.slice()
                plainText.limit(plainTextLength)
                onPlainText?.invoke(plainText.asReadOnlyBuffer())
                buffer.position(buffer.position() + plainTextLength)
            }
        }
        if (lines.isEmpty()) return null
        return SyncedLyrics(lines = lines, title = title, id = id, artists = artists)
    }

    private fun ByteBuffer.readLine(): ISyncedLine = when (get().toInt()) {
        0 -> SyncedLine(
            content = readString(),
            translation = readOptionalString(),
            start = int,
            end = int,
        )
        1 -> KaraokeLine.MainKaraokeLine(
            syllables = readSyllables(),
            translation = readOptionalString(),
            alignment = readAlignment(),
            start = int,
            end = int,
            phonetic = readOptionalString(),
            accompanimentLines = List(readCount()) { readAccompaniment() }.takeIf { it.isNotEmpty() },
        )
        2 -> readAccompaniment()
        else -> error("Unknown native lyrics line type")
    }

    private fun ByteBuffer.readAccompaniment() = KaraokeLine.AccompanimentKaraokeLine(
        syllables = readSyllables(),
        translation = readOptionalString(),
        alignment = readAlignment(),
        start = int,
        end = int,
        phonetic = readOptionalString(),
    )

    private fun ByteBuffer.readSyllables(): List<KaraokeSyllable> = List(readCount()) {
        KaraokeSyllable(
            content = readString(),
            start = int,
            end = int,
            phonetic = readOptionalString(),
        )
    }

    private fun ByteBuffer.readAlignment(): KaraokeAlignment = when (get().toInt()) {
        0 -> KaraokeAlignment.Start
        1 -> KaraokeAlignment.End
        else -> KaraokeAlignment.Unspecified
    }

    private fun ByteBuffer.readCount(): Int = int.also {
        require(it in 0..1_000_000) { "Invalid native lyrics count" }
    }

    private fun ByteBuffer.readString(): String {
        val length = readCount()
        require(length <= remaining()) { "Truncated native lyrics string" }
        val slice = slice()
        slice.limit(length)
        position(position() + length)
        return StandardCharsets.UTF_8.decode(slice).toString()
    }

    private fun ByteBuffer.readOptionalString(): String? {
        val length = int
        if (length < 0) return null
        require(length <= remaining()) { "Truncated native lyrics string" }
        val slice = slice()
        slice.limit(length)
        position(position() + length)
        return StandardCharsets.UTF_8.decode(slice).toString()
    }

    private external fun nativeParseToWire(content: String): ByteBuffer?
    private external fun nativeParseToPlainText(content: String): ByteBuffer?
    private external fun nativeParseFdToWire(fd: Int): ByteBuffer?
    private external fun nativeParseFdToPlainText(fd: Int): ByteBuffer?
    private external fun nativeReleaseBuffer(buffer: ByteBuffer)
}
