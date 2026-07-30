package com.mocharealm.accompanist.lyrics.text

import com.mocharealm.accompanist.lyrics.core.model.Artist
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeSyllable
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import java.io.ByteArrayOutputStream
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

    /** Decode text_engine's stable LYR1 representation without parsing source text again. */
    fun decodeWire(bytes: ByteArray): SyncedLyrics? = runCatching {
        decodeLyrics(ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN), null)
    }.getOrNull()

    /**
     * Encode an already parsed lyrics tree into text_engine's stable LYR1 wire format.
     * This preserves karaoke syllables, accompaniment, phonetics and exact timings.
     */
    fun encodeWire(lyrics: SyncedLyrics): ByteArray {
        val out = ByteArrayOutputStream()
        out.write(byteArrayOf('L'.code.toByte(), 'Y'.code.toByte(), 'R'.code.toByte(), '1'.code.toByte()))
        out.putString(lyrics.title)
        out.putString(lyrics.id)
        val artists = lyrics.artists.orEmpty()
        out.putIntLe(artists.size)
        artists.forEach { artist ->
            out.putString(artist.type)
            out.putString(artist.name)
        }
        out.putIntLe(lyrics.lines.size)
        lyrics.lines.forEach { line ->
            when (line) {
                is SyncedLine -> {
                    out.write(0)
                    out.putString(line.content)
                    out.putOptionalString(line.translation)
                    out.putIntLe(line.start)
                    out.putIntLe(line.end)
                }
                is KaraokeLine.MainKaraokeLine -> {
                    out.write(1)
                    out.putSyllables(line.syllables)
                    out.putOptionalString(line.translation)
                    out.putAlignment(line.alignment)
                    out.putIntLe(line.start)
                    out.putIntLe(line.end)
                    out.putOptionalString(line.phonetic)
                    val accompaniment = line.accompanimentLines.orEmpty()
                    out.putIntLe(accompaniment.size)
                    accompaniment.forEach { out.putAccompaniment(it) }
                }
                is KaraokeLine.AccompanimentKaraokeLine -> {
                    out.write(2)
                    out.putAccompaniment(line)
                }
            }
        }
        // The final field is the optional normalized search-text trailer.
        out.putIntLe(0)
        return out.toByteArray()
    }

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

    private fun ByteArrayOutputStream.putIntLe(value: Int) {
        write(value and 0xff)
        write((value ushr 8) and 0xff)
        write((value ushr 16) and 0xff)
        write((value ushr 24) and 0xff)
    }

    private fun ByteArrayOutputStream.putString(value: String) {
        val bytes = value.toByteArray(StandardCharsets.UTF_8)
        putIntLe(bytes.size)
        write(bytes)
    }

    private fun ByteArrayOutputStream.putOptionalString(value: String?) {
        if (value == null) putIntLe(-1) else putString(value)
    }

    private fun ByteArrayOutputStream.putAlignment(value: KaraokeAlignment) {
        write(
            when (value) {
                KaraokeAlignment.Start -> 0
                KaraokeAlignment.End -> 1
                KaraokeAlignment.Unspecified -> 2
            },
        )
    }

    private fun ByteArrayOutputStream.putSyllables(syllables: List<KaraokeSyllable>) {
        putIntLe(syllables.size)
        syllables.forEach { syllable ->
            putString(syllable.content)
            putIntLe(syllable.start)
            putIntLe(syllable.end)
            putOptionalString(syllable.phonetic)
        }
    }

    private fun ByteArrayOutputStream.putAccompaniment(
        line: KaraokeLine.AccompanimentKaraokeLine,
    ) {
        putSyllables(line.syllables)
        putOptionalString(line.translation)
        putAlignment(line.alignment)
        putIntLe(line.start)
        putIntLe(line.end)
        putOptionalString(line.phonetic)
    }

    private external fun nativeParseToWire(content: String): ByteBuffer?
    private external fun nativeParseToPlainText(content: String): ByteBuffer?
    private external fun nativeParseFdToWire(fd: Int): ByteBuffer?
    private external fun nativeParseFdToPlainText(fd: Int): ByteBuffer?
    private external fun nativeReleaseBuffer(buffer: ByteBuffer)
}
