package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import android.content.Context
import android.graphics.fonts.Font
import android.graphics.fonts.FontStyle
import android.graphics.fonts.SystemFonts
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontListFontFamily
import com.mocharealm.accompanist.lyrics.text.NativeFontAxis
import com.mocharealm.accompanist.lyrics.text.NativeFontSource
import org.lsposed.hiddenapibypass.HiddenApiBypass

@Composable
actual fun getPlatformContext(): Any? {
    return LocalContext.current
}

actual fun getFontSource(fontFamily: FontFamily?, platformContext: Any?): NativeFontSource? {
    val context = platformContext as? Context

    if (context != null && fontFamily is FontListFontFamily) {
        val font = fontFamily.fonts.firstOrNull()
        if (font != null) {
            readResourceFont(context, font)?.let { return it }
            readAssetFont(context, font)?.let { return it }
        }
    }

    return getAndroidDefaultFontSource()
}

actual fun getSystemFallbackFontSources(platformContext: Any?): List<NativeFontSource> {
    return androidSystemFontSources
}

private val androidSystemFontSources: List<NativeFontSource> by lazy {
    readAndroidSystemFonts()
        .dedupeByFontFace()
}

private fun readAndroidSystemFonts(): List<NativeFontSource> {
    return readAndroidFallbackFontsFromSystemFallback()
        .ifEmpty { readAvailableAndroidFonts() }
}

private fun readAndroidFallbackFontsFromSystemFallback(): List<NativeFontSource> {
    return try {
        val systemFontsClass = Class.forName("android.graphics.fonts.SystemFonts")
        val configMethodNames = listOf(
            "getSystemPreinstalledFontConfigFromLegacyXml",
            "getSystemPreinstalledFontConfig"
        )

        for (methodName in configMethodNames) {
            val fontConfig = try {
                HiddenApiBypass.invoke(systemFontsClass, null, methodName)
            } catch (_: Throwable) {
                null
            } ?: continue
            val sources = readAndroidFallbackFontsFromFontConfig(systemFontsClass, fontConfig)
            if (sources.isNotEmpty()) {
                return sources
            }
        }
        emptyList()
    } catch (_: Throwable) {
        emptyList()
    }
}

private fun readAndroidFallbackFontsFromFontConfig(
    systemFontsClass: Class<*>,
    fontConfig: Any
): List<NativeFontSource> {
    val fallback = try {
        HiddenApiBypass.invoke(
            systemFontsClass,
            null,
            "buildSystemFallback",
            fontConfig
        ) as? Map<*, *>
    } catch (_: Throwable) {
        null
    } ?: return emptyList()

    val familyNames = orderedAndroidFamilyNames(fallback)
    return familyNames.flatMap { familyName ->
        fallback[familyName]
            .asFontFamilySequence()
            .flatMap { fontFamily ->
                readFontsFromAndroidFontFamily(fontFamily, familyName).asSequence()
            }
            .toList()
    }
}

private fun readAvailableAndroidFonts(): List<NativeFontSource> {
    return try {
        SystemFonts.getAvailableFonts().mapNotNull { font ->
            font.toNativeFontSource(fallbackFor = null)
        }
    } catch (_: Throwable) {
        emptyList()
    }
}

private fun orderedAndroidFamilyNames(fallback: Map<*, *>): List<String> {
    val keys = fallback.keys.mapNotNull { it as? String }
    val preferred = listOf("sans-serif", "serif", "monospace")
    return buildList {
        preferred.forEach { name ->
            if (name in keys) add(name)
        }
        keys.forEach { name ->
            if (name !in this) add(name)
        }
    }
}

private fun Any?.asFontFamilySequence(): Sequence<Any> {
    return when {
        this == null -> emptySequence()
        this is Array<*> -> asSequence().filterNotNull()
        this is Iterable<*> -> asSequence().filterNotNull()
        javaClass.isArray -> (0 until java.lang.reflect.Array.getLength(this))
            .asSequence()
            .mapNotNull { index -> java.lang.reflect.Array.get(this, index) }
        else -> emptySequence()
    }
}

private fun readFontsFromAndroidFontFamily(fontFamily: Any, familyName: String): List<NativeFontSource> {
    return try {
        val size = fontFamily.javaClass.getMethod("getSize").invoke(fontFamily) as? Int
            ?: return emptyList()
        val getFont = fontFamily.javaClass.getMethod("getFont", Int::class.javaPrimitiveType)
        (0 until size).mapNotNull { index ->
            (getFont.invoke(fontFamily, index) as? Font)
                ?.toNativeFontSource(fallbackFor = familyName)
        }
    } catch (_: Throwable) {
        emptyList()
    }
}

private fun Font.toNativeFontSource(fallbackFor: String?): NativeFontSource? {
    val file = file ?: return null
    if (!file.exists() || !file.canRead()) return null

    return NativeFontSource(
        path = file.absolutePath,
        ttcIndex = ttcIndex,
        weight = style.weight,
        italic = style.slant == FontStyle.FONT_SLANT_ITALIC,
        axes = axes?.mapNotNull { axis ->
            val tag = axis.tag ?: return@mapNotNull null
            if (tag.length == 4) NativeFontAxis(tag, axis.styleValue) else null
        }.orEmpty(),
        languageTags = localeList.toLanguageTags()
            .split(",")
            .map { it.trim() }
            .filter { it.isNotEmpty() },
        fallbackFor = fallbackFor,
        sourceId = buildString {
            append("android-system:")
            if (fallbackFor != null) {
                append(fallbackFor)
                append(":")
            }
            append(file.absolutePath)
            append("#")
            append(ttcIndex)
        }
    )
}

private fun getAndroidDefaultFontSource(): NativeFontSource? {
    fun NativeFontSource.isNormalUpright(): Boolean {
        return weight == FontStyle.FONT_WEIGHT_NORMAL && !italic
    }

    return androidSystemFontSources.firstOrNull { source ->
        source.fallbackFor == "sans-serif" && source.isNormalUpright() && source.languageTags.isEmpty()
    } ?: androidSystemFontSources.firstOrNull { source ->
        source.fallbackFor == "sans-serif" && source.isNormalUpright()
    } ?: androidSystemFontSources.firstOrNull { source ->
        source.isNormalUpright() &&
            listOfNotNull(source.path, source.sourceId).any { it.contains("Roboto", ignoreCase = true) }
    } ?: androidSystemFontSources.firstOrNull { source ->
        source.isNormalUpright() && source.languageTags.isEmpty()
    } ?: androidSystemFontSources.firstOrNull()
}

private fun readResourceFont(context: Context, font: androidx.compose.ui.text.font.Font): NativeFontSource? {
    return try {
        val resIdField = font.javaClass.getDeclaredField("resId")
        resIdField.isAccessible = true
        val resId = resIdField.getInt(font)
        if (resId != 0) {
            NativeFontSource(
                resourceId = resId,
                sourceId = "android-resource:${context.packageName}:$resId",
                platformContext = context.applicationContext
            )
        } else {
            null
        }
    } catch (_: Throwable) {
        null
    }
}

private fun readAssetFont(context: Context, font: androidx.compose.ui.text.font.Font): NativeFontSource? {
    return try {
        val pathField = font.javaClass.getDeclaredField("path")
        pathField.isAccessible = true
        val path = pathField.get(font) as? String ?: return null
        NativeFontSource(
            assetPath = path,
            sourceId = "android-asset:${context.packageName}:$path",
            platformContext = context.applicationContext
        )
    } catch (_: Throwable) {
        null
    }
}

private fun List<NativeFontSource>.dedupeByFontFace(): List<NativeFontSource> {
    val seen = mutableSetOf<String>()
    return filter { source ->
        val path = source.path ?: return@filter true
        val axisKey = source.axes.joinToString(";") { "${it.tag}:${it.value}" }
        seen.add("$path#${source.ttcIndex}#${source.weight}#${source.italic}#$axisKey")
    }
}
