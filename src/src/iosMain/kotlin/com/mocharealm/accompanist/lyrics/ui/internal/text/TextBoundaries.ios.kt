@file:OptIn(kotlinx.cinterop.ExperimentalForeignApi::class)

package com.mocharealm.accompanist.lyrics.ui.internal.text

import com.mocharealm.accompanist.lyrics.ui.icu.*
import kotlinx.cinterop.*
import platform.CoreFoundation.*
import platform.Foundation.*

private fun boundaries(text: String, localeTag: String?, unit: ULong): IntArray = memScoped {
    if (text.isEmpty()) return@memScoped intArrayOf(0)
    val characters = allocArray<UShortVar>(text.length + 1)
    text.forEachIndexed { index, character -> characters[index] = character.code.toUShort() }
    val string = checkNotNull(CFStringCreateWithCharacters(kCFAllocatorDefault, characters, text.length.toLong()))
    val localeName = CFStringCreateWithCString(kCFAllocatorDefault, localeTag ?: "", kCFStringEncodingUTF8)
    val locale = CFLocaleCreate(kCFAllocatorDefault, localeName)
    try {
        val tokenizer = checkNotNull(CFStringTokenizerCreate(kCFAllocatorDefault, string,
            CFRangeMake(0, text.length.toLong()), unit, locale))
        try {
            val result = arrayListOf(0, text.length)
            while (CFStringTokenizerAdvanceToNextToken(tokenizer) != kCFStringTokenizerTokenNone) {
                CFStringTokenizerGetCurrentTokenRange(tokenizer).useContents {
                    if (location >= 0) {
                        result.add(location.toInt())
                        result.add((location + length).toInt())
                    }
                }
            }
            result.distinct().sorted().toIntArray()
        } finally { CFRelease(tokenizer) }
    } finally {
        if (locale != null) CFRelease(locale)
        if (localeName != null) CFRelease(localeName)
        CFRelease(string)
    }
}

internal actual fun platformLineBreakBoundaries(text: String, localeTag: String?): IntArray =
    boundaries(text, localeTag ?: NSLocale.currentLocale.localeIdentifier, kCFStringTokenizerUnitLineBreak)

internal actual fun platformWordBreakBoundaries(text: String, localeTag: String?): IntArray =
    boundaries(text, localeTag, kCFStringTokenizerUnitWordBoundary)

internal actual fun graphemeBoundaries(text: String): BooleanArray {
    val result = BooleanArray(text.length + 1)
    result[0] = true
    val string = NSString.create(string = text)
    var index = 0
    while (index < text.length) {
        index = string.rangeOfComposedCharacterSequenceAtIndex(index.toULong()).useContents {
            (location + length).toInt()
        }
        result[index] = true
    }
    return result
}

internal actual fun platformCodePointDirectionality(codePoint: Int): Int = u_charDirection(codePoint).toInt()

internal actual fun Char.isCjk(): Boolean = code in 0x3400..0x4DBF || code in 0x4E00..0x9FFF ||
    code in 0xF900..0xFAFF || code in 0x3000..0x30FF || code in 0x1100..0x11FF ||
    code in 0x3130..0x318F || code in 0xAC00..0xD7AF

internal actual fun Char.isArabic(): Boolean = code in 0x0600..0x06FF || code in 0x0750..0x077F ||
    code in 0x0870..0x08FF || code in 0xFB50..0xFDFF || code in 0xFE70..0xFEFF

internal actual fun Char.isDevanagari(): Boolean = code in 0x0900..0x097F || code in 0xA8E0..0xA8FF
