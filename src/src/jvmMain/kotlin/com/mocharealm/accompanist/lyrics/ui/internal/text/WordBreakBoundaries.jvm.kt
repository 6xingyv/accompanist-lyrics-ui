package com.mocharealm.accompanist.lyrics.ui.internal.text

import com.ibm.icu.text.BreakIterator
import java.util.Locale

internal actual fun platformWordBreakBoundaries(text: String, localeTag: String?): IntArray {
    val locale = localeTag?.let(Locale::forLanguageTag) ?: Locale.ROOT
    val iterator = BreakIterator.getWordInstance(locale)
    iterator.setText(text)
    val boundaries = ArrayList<Int>()
    var boundary = iterator.first()
    while (boundary != BreakIterator.DONE) {
        boundaries.add(boundary)
        boundary = iterator.next()
    }
    return boundaries.toIntArray()
}
