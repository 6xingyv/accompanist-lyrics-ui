package com.mocharealm.accompanist.sample.data.utils

import android.content.Context
import android.icu.text.Transliterator
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.PhoneticLevel
import com.mocharealm.accompanist.lyrics.core.utils.*
import com.mocharealm.accompanist.lyrics.phonetics.*
import com.mocharealm.accompanist.lyrics.phonetics.data.*

/** Shared read-only packs; each request uses an independent workspace. */
object AndroidPhoneticProvider : PhoneticProvider {
    @Volatile private var loader: PackLoader? = null
    fun initialize(context: Context) = initialize(AndroidPackLoader(context))
    internal fun initialize(packLoader: PackLoader) {
        synchronized(this) { if (loader == null) loader = SharedPackLoader(packLoader) }
    }
    private val korean by lazy { Transliterator.getInstance("Hangul-Latin; Latin-ASCII") }
    private val delegate by lazy {
        createSamplePhonetics(checkNotNull(loader) { "Initialize AndroidPhoneticProvider with application context" }) { text ->
            synchronized(korean) { korean.transliterate(text).lowercase(java.util.Locale.ROOT) }
        }
    }
    override val phoneticLevel = PhoneticLevel.LINE
    override fun getPhonetic(string: String): String = delegate.getPhonetic(string)
    override fun getPhonetic(string: String, languageTag: String?): String = delegate.getPhonetic(string, languageTag)
    override fun resolve(request: PhoneticRequest): PhoneticResponse = delegate.resolve(request)
    fun enrich(lyrics: SyncedLyrics): SyncedLyrics = lyrics.withPhonetics(this)
    /** Optional song/editor readings; supplied file captions retain core's existing priority. */
    fun withReadings(readings: (String) -> List<ReadingOverride>): PhoneticProvider =
        createSamplePhonetics(checkNotNull(loader), readings) { text ->
            synchronized(korean) { korean.transliterate(text).lowercase(java.util.Locale.ROOT) }
        }
}

/** Thin core-to-phonetics adapter. Script dispatch and regional policy belong to the profiles. */
internal class ProfilePhoneticProvider(
    private val engine: PhoneticEngine,
    private val formatter: AsciiFormatter,
    private val readings: (String) -> List<ReadingOverride> = { emptyList() },
) : PhoneticProvider {
    override val phoneticLevel = PhoneticLevel.LINE
    override fun getPhonetic(string: String): String = getPhonetic(string, null)
    override fun getPhonetic(string: String, languageTag: String?): String {
        val hints = languageTag?.takeIf { string.isNotEmpty() }?.let {
            listOf(PhoneticLanguageHint(PhoneticTextRange(0, string.length), it))
        }.orEmpty()
        return resolve(PhoneticRequest(string, hints)).phonetic.orEmpty()
    }
    override fun resolve(request: PhoneticRequest): PhoneticResponse {
        val parsed = engine.parse(request.text, ParseOptions(hints = request.hints.map {
            LanguageHint(TextRange(it.range.start, it.range.end), it.languageTag)
        }, overrides = readings(request.text)))
        val ranges = request.projectionRanges
        val completePartition = ranges.firstOrNull()?.start == 0 && ranges.lastOrNull()?.end == request.text.length &&
            ranges.zipWithNext().all { (a, b) -> a.end == b.start }
        val projections = if (ranges.isEmpty()) emptyList() else if (!completePartition) {
            ranges.map {
                val latin = formatter.formatRange(parsed, TextRange(it.start, it.end))
                PhoneticProjection(latin?.ifBlank { null }, aligned = latin != null)
            }
        } else {
            formatter.formatRanges(parsed, request.projectionRanges.map { TextRange(it.start, it.end) })?.map {
                PhoneticProjection(it.latin.ifBlank { null }, separatorBefore = it.separatorBefore)
            } ?: request.projectionRanges.map { PhoneticProjection(null, aligned = false) }
        }
        return PhoneticResponse(
            formatter.format(parsed).latin.ifBlank { null },
            projections,
        )
    }
    fun enrich(lyrics: SyncedLyrics): SyncedLyrics = lyrics.withPhonetics(this)
}

internal fun createSamplePhonetics(packs: PackLoader, readings: (String) -> List<ReadingOverride> = { emptyList() }, romanizeHangul: (String) -> String): ProfilePhoneticProvider {
    val cjk = CjkPhoneticProfile(listOf(
        MandarinData.mainland(packs), MandarinData.taiwan(packs), CantoneseData.hongKong(packs), JapaneseData.japanese(packs),
    ), defaultLocale = ReadingLocale.MANDARIN_CN)
    return ProfilePhoneticProvider(
        PhoneticEngine(profiles = listOf(cjk, SampleHangulProfile)),
        AsciiFormatter(FormatOptions(passthrough = PassthroughStyle.OMIT), formatters = listOf(object : PronunciationFormatter {
            override val notation = "sample-hangul"
            override fun format(pronunciation: Pronunciation, options: FormatOptions): String =
                romanizeHangul(pronunciation.units.joinToString("") { it.value })
        })),
        readings,
    )
}

/** The sample's legacy ICU Korean Romanization remains an optional extension, with no bundled pack.
 * Preserve a complete Hangul run as native notation. ICU is called only by the separate formatter.
 * It has no proven per-character alignment, so partial projections use the line caption fallback.
 */
private object SampleHangulProfile : PhoneticProfile {
    override fun matches(codePoint: Int): Boolean = isHangulCodePoint(codePoint)
    override fun resolve(text: String, range: TextRange, options: ParseOptions, workspace: ParseWorkspace): LanguageResult {
        val reading = ReadingCandidate(
            Pronunciation(null, listOf(ReadingUnit(range.textIn(text), sourceRange = range)), notation = "sample-hangul", languageTag = "ko"),
            sourceIds = listOf("sample-android-icu"),
        )
        return LanguageResult(listOf(PhoneticSpan(range, null, Resolution.RESOLVED, reading, listOf(reading))))
    }
}
