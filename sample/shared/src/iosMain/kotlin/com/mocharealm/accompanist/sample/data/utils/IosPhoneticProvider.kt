@file:OptIn(kotlinx.cinterop.ExperimentalForeignApi::class)

package com.mocharealm.accompanist.sample.data.utils

import com.mocharealm.accompanist.lyrics.core.utils.PhoneticProvider
import com.mocharealm.accompanist.lyrics.phonetics.IosBundlePackLoader
import com.mocharealm.accompanist.lyrics.phonetics.SharedPackLoader
import platform.Foundation.NSBundle
import kotlinx.cinterop.*
import platform.CoreFoundation.*

internal val IosPhoneticProvider: PhoneticProvider by lazy {
    val resources = checkNotNull(NSBundle.mainBundle.resourcePath)
    val bundle = checkNotNull(NSBundle.bundleWithPath("$resources/phonetics.bundle")) {
        "Missing bundled pronunciation dictionaries"
    }
    createSamplePhonetics(SharedPackLoader(IosBundlePackLoader(bundle)),
        romanizeHangul = ::romanizeHangul)
}

private fun romanizeHangul(text: String): String = memScoped {
    val value = checkNotNull(CFStringCreateMutable(null, 0))
    try {
        CFStringAppendCString(value, text, kCFStringEncodingUTF8)
        val transform = checkNotNull(CFStringCreateWithCString(null,
            "Hangul-Latin; Latin-ASCII", kCFStringEncodingUTF8))
        try {
            check(CFStringTransform(value, null, transform, false))
            val length = CFStringGetLength(value).toInt()
            val characters = allocArray<UShortVar>(length)
            CFStringGetCharacters(value, CFRangeMake(0, length.toLong()), characters)
            buildString(length) { repeat(length) { append(characters[it].toInt().toChar()) } }
                .lowercase()
        } finally { CFRelease(transform) }
    } finally { CFRelease(value) }
}
