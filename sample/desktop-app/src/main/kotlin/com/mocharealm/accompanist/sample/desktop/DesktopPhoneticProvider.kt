package com.mocharealm.accompanist.sample.desktop

import com.ibm.icu.text.Transliterator
import com.mocharealm.accompanist.lyrics.core.model.karaoke.PhoneticLevel
import com.mocharealm.accompanist.lyrics.core.utils.PhoneticProvider
import com.mocharealm.accompanist.lyrics.core.utils.PhoneticRequest
import com.mocharealm.accompanist.lyrics.core.utils.PhoneticResponse
import com.mocharealm.accompanist.lyrics.phonetics.ClasspathPackLoader
import com.mocharealm.accompanist.lyrics.phonetics.SharedPackLoader
import com.mocharealm.accompanist.sample.data.utils.createSamplePhonetics
import java.util.Locale

internal object DesktopPhoneticProvider : PhoneticProvider {
    private val korean by lazy { Transliterator.getInstance("Hangul-Latin; Latin-ASCII") }
    private val delegate by lazy {
        createSamplePhonetics(SharedPackLoader(ClasspathPackLoader())) { text ->
            synchronized(korean) { korean.transliterate(text).lowercase(Locale.ROOT) }
        }
    }

    override val phoneticLevel = PhoneticLevel.LINE
    override fun getPhonetic(string: String): String = delegate.getPhonetic(string)
    override fun getPhonetic(string: String, languageTag: String?): String =
        delegate.getPhonetic(string, languageTag)
    override fun resolve(request: PhoneticRequest): PhoneticResponse = delegate.resolve(request)
}
