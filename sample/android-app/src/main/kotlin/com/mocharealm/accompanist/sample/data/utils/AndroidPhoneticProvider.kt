package com.mocharealm.accompanist.sample.data.utils

import android.content.Context
import android.icu.text.Transliterator
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.PhoneticLevel
import com.mocharealm.accompanist.lyrics.core.utils.*
import com.mocharealm.accompanist.lyrics.phonetics.*

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
