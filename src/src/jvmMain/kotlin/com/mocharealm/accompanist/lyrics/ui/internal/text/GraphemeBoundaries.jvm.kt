package com.mocharealm.accompanist.lyrics.ui.internal.text

private val graphemePattern = java.util.regex.Pattern.compile("\\X")

internal actual fun graphemeBoundaries(text: String): BooleanArray {
    val result = BooleanArray(text.length + 1)
    result[0] = true
    val matcher = graphemePattern.matcher(text)
    while (matcher.find()) result[matcher.end()] = true
    return result
}
