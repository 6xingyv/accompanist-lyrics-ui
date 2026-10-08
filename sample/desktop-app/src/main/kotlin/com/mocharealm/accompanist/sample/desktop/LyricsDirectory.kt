package com.mocharealm.accompanist.sample.desktop

import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.parser.AutoParser
import java.nio.file.Files
import java.nio.file.Path
import java.text.Normalizer
import java.util.Locale
import kotlin.io.path.extension
import kotlin.io.path.nameWithoutExtension
import kotlin.io.path.readBytes
import java.io.ByteArrayInputStream
import java.util.zip.InflaterInputStream

internal class LyricsDirectory(private val root: Path, private val recursive: Boolean) {
    private var files = emptyList<Pair<String, Path>>()
    private var scannedAt = 0L
    private val indexRevision = java.util.concurrent.atomic.AtomicLong()
    private var scannedRevision = -1L

    fun invalidate() { indexRevision.incrementAndGet() }

    @Synchronized fun rescan() {
        invalidate()
        scan(monotonicMillis(), indexRevision.get())
    }

    private fun scan(now: Long, revision: Long) {
        files = Files.walk(root, if (recursive) Int.MAX_VALUE else 1).use { paths ->
            paths.filter { Files.isRegularFile(it) && it.extension.lowercase(Locale.ROOT) in
                setOf("ttml", "xml", "lrc", "elrc", "lys", "yrc", "krc") }
                .sorted().map { normalize(it.nameWithoutExtension) to it }.toList()
        }
        scannedAt = now
        scannedRevision = revision
    }

    @Synchronized fun match(title: String, artist: String): Path? {
        val now = monotonicMillis()
        val revision = indexRevision.get()
        if (scannedRevision != revision || scannedAt == 0L || now - scannedAt > 10_000L) {
            scan(now, revision)
        }
        val wanted = listOf(title, "$artist - $title", "$title - $artist")
            .map(::normalize).filter { it.isNotBlank() }
        val wantedArtist = normalize(artist)
        return files.map { (name, path) ->
            val titleScore = wanted.maxOfOrNull { score(it, name) }.orEmptyScore()
            val artistMatches = wantedArtist.isNotEmpty() && root.relativize(path).parent
                ?.any { normalize(it.toString()).contains(wantedArtist) } == true
            (titleScore + if (titleScore >= 65 && artistMatches) 20 else 0) to path
        }
            .filter { it.first >= 65 }.maxByOrNull { it.first }?.second
    }

    private fun Int?.orEmptyScore() = this ?: 0
    private fun normalize(text: String) = Normalizer.normalize(text, Normalizer.Form.NFKC)
        .lowercase(Locale.ROOT).filter { it.isLetterOrDigit() }

    private fun score(expected: String, candidate: String): Int {
        if (expected == candidate) return 100
        if (expected.isEmpty() || candidate.isEmpty()) return 0
        if (candidate.endsWith(expected) && candidate.removeSuffix(expected).all { it.isDigit() })
            return 90 // Track-number prefix, not a different title ending in this word.
        if (candidate.contains(expected) || expected.contains(candidate)) {
            val ratio = minOf(expected.length, candidate.length).toFloat() / maxOf(expected.length, candidate.length)
            return (ratio * 90).toInt()
        }
        val previous = IntArray(candidate.length + 1)
        for (left in expected) {
            var diagonal = 0
            for (index in candidate.indices) {
                val old = previous[index + 1]
                previous[index + 1] = if (left == candidate[index]) diagonal + 1
                    else maxOf(previous[index], previous[index + 1])
                diagonal = old
            }
        }
        return 200 * previous.last() / (expected.length + candidate.length)
    }
}

internal fun parseDesktopLyrics(path: Path): SyncedLyrics {
    require(Files.size(path) <= 16 * 1024 * 1024) { "Lyrics file exceeds 16 MB" }
    val encoded = path.readBytes()
    val content = if (encoded.size >= 4 && encoded.take(4).toByteArray().contentEquals("krc1".toByteArray())) {
        val key = byteArrayOf(64, 71, 97, 119, 94, 50, 116, 71, 81, 54, 49, 45, -50, -46, 110, 105)
        val compressed = ByteArray(encoded.size - 4) { index ->
            (encoded[index + 4].toInt() xor key[index % key.size].toInt()).toByte()
        }
        InflaterInputStream(ByteArrayInputStream(compressed)).use {
            val bytes = it.readNBytes(16 * 1024 * 1024 + 1)
            require(bytes.size <= 16 * 1024 * 1024) { "Decompressed lyrics exceed 16 MB" }
            bytes.toString(Charsets.UTF_8)
        }
    } else encoded.toString(Charsets.UTF_8)
    return AutoParser().parse(content.removePrefix("\uFEFF")).also {
        require(it.lines.isNotEmpty()) { "No supported timed lyrics in ${path.fileName}" }
    }
}

internal fun monotonicMillis(): Long = System.nanoTime() / 1_000_000L
