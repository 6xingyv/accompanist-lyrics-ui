package com.mocharealm.accompanist.lyrics.ui.renderer

import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine

data class NativeLyricsRendererStyle(
    val normalFontSizePx: Float,
    val normalLineHeightPx: Float,
    val accompanimentFontSizePx: Float,
    val accompanimentLineHeightPx: Float,
    val translationFontSizePx: Float,
    val translationLineHeightPx: Float,
    val phoneticFontSizePx: Float = translationFontSizePx,
    val phoneticLineHeightPx: Float = translationLineHeightPx,
    val phoneticGapPx: Float = 4f,
    val paddingXPx: Float,
    val paddingYPx: Float,
    val keepAlivePx: Float,
    val textColorArgb: Int,
    val showTranslation: Boolean = true,
    val showPhonetic: Boolean = true,
    val useBlurEffect: Boolean = true,
    val blurDelta: Float = 3f,
    val breathingDotsNumber: Int = 3,
    val breathingDotsSizePx: Float = 16f,
    val breathingDotsMarginPx: Float = 12f,
    val breathingDotsEnterMs: Int = 3000,
    val breathingDotsStillMs: Int = 200,
    val breathingDotsDipMs: Int = 3000,
    val breathingDotsExitMs: Int = 200,
    val breathingDotsColorArgb: Int = textColorArgb
)

fun SyncedLyrics.toNativeLyricsSceneJson(
    widthPx: Int,
    heightPx: Int,
    style: NativeLyricsRendererStyle
): String {
    return buildString {
        append('{')
        appendJsonField("width", widthPx)
        append(',')
        appendJsonField("height", heightPx)
        append(',')
        appendJsonField("locale", detectNativeLyricsLocale())
        append(',')
        appendJsonField("normal_font_size", style.normalFontSizePx)
        append(',')
        appendJsonField("normal_line_height", style.normalLineHeightPx)
        append(',')
        appendJsonField("accompaniment_font_size", style.accompanimentFontSizePx)
        append(',')
        appendJsonField("accompaniment_line_height", style.accompanimentLineHeightPx)
        append(',')
        appendJsonField("translation_font_size", style.translationFontSizePx)
        append(',')
        appendJsonField("translation_line_height", style.translationLineHeightPx)
        append(',')
        appendJsonField("phonetic_font_size", style.phoneticFontSizePx)
        append(',')
        appendJsonField("phonetic_line_height", style.phoneticLineHeightPx)
        append(',')
        appendJsonField("phonetic_gap", style.phoneticGapPx)
        append(',')
        appendJsonField("padding_x", style.paddingXPx)
        append(',')
        appendJsonField("padding_y", style.paddingYPx)
        append(',')
        appendJsonField("keep_alive", style.keepAlivePx)
        append(',')
        appendJsonField("text_color", style.textColorArgb.toUInt().toLong())
        append(',')
        appendJsonField("show_translation", style.showTranslation)
        append(',')
        appendJsonField("show_phonetic", style.showPhonetic)
        append(',')
        appendJsonField("use_blur_effect", style.useBlurEffect)
        append(',')
        appendJsonField("blur_delta", style.blurDelta)
        append(',')
        appendJsonField("breathing_dots_number", style.breathingDotsNumber)
        append(',')
        appendJsonField("breathing_dots_size", style.breathingDotsSizePx)
        append(',')
        appendJsonField("breathing_dots_margin", style.breathingDotsMarginPx)
        append(',')
        appendJsonField("breathing_dots_enter_ms", style.breathingDotsEnterMs)
        append(',')
        appendJsonField("breathing_dots_still_ms", style.breathingDotsStillMs)
        append(',')
        appendJsonField("breathing_dots_dip_ms", style.breathingDotsDipMs)
        append(',')
        appendJsonField("breathing_dots_exit_ms", style.breathingDotsExitMs)
        append(',')
        appendJsonField("breathing_dots_color", style.breathingDotsColorArgb.toUInt().toLong())
        append(',')
        append("\"lines\":[")
        var emittedLine = false
        fun appendSceneLine(
            line: ISyncedLine,
            sourceIndex: Int,
            clusterIndex: Int,
            clusterRole: String
        ) {
            if (emittedLine) append(',')
            appendLineJson(line, sourceIndex, clusterIndex, clusterRole)
            emittedLine = true
        }
        lines.forEachIndexed { index, line ->
            if (line is KaraokeLine.MainKaraokeLine) {
                val accompanimentLines = line.accompanimentLines.orEmpty()
                accompanimentLines
                    .filter { it.start < line.start }
                    .forEach { appendSceneLine(it, index, index, "before_accompaniment") }
                appendSceneLine(line, index, index, "main")
                accompanimentLines
                    .filter { it.start >= line.start }
                    .forEach { appendSceneLine(it, index, index, "after_accompaniment") }
            } else if (line !is KaraokeLine.AccompanimentKaraokeLine) {
                appendSceneLine(line, index, index, "standalone")
            }
        }
        append(']')
        append('}')
    }
}

fun SyncedLyrics.detectNativeLyricsLocale(): String {
    val text = buildString {
        lines.forEach { line ->
            when (line) {
                is KaraokeLine -> {
                    line.syllables.forEach { append(it.content) }
                    line.translation?.let(::append)
                    line.phonetic?.let(::append)
                    if (line is KaraokeLine.MainKaraokeLine) {
                        line.accompanimentLines.orEmpty().forEach { accompaniment ->
                            accompaniment.syllables.forEach { append(it.content) }
                        }
                    }
                }
                is SyncedLine -> {
                    append(line.content)
                    line.translation?.let(::append)
                }
                else -> Unit
            }
        }
    }

    if (text.any { it.code in 0x3040..0x30ff }) return "ja-JP"
    if (text.any { it.code in 0xac00..0xd7af }) return "ko-KR"

    val hasHan = text.any { it.code in 0x3400..0x9fff || it.code in 0xf900..0xfaff }
    if (!hasHan) return "en-US"

    val simplifiedMarkers = "们这为汉说会过个后里时没话听讲台词假谎剧镜头责杀爱赶给结局王乞丐众都少羡慕将轻说放伤灯塔让捧算码终学会筹你看停只剩泥洼夏营结束们走散犹豫曾拨锅虾透挣扎应该留盛世界么并特别还梦问"
    val traditionalMarkers = "們這為漢說會過個後裡時沒話聽講臺詞假謊劇鏡頭責殺愛趕給結局王乞丐眾都少羨慕將輕說放傷燈塔讓捧算碼終學會籌你看停只剩泥窪夏營結束們走散猶豫曾撥鍋蝦透掙扎應該留盛世界麼並特別還夢問"
    val simplifiedScore = text.count { it in simplifiedMarkers }
    val traditionalScore = text.count { it in traditionalMarkers }

    return if (traditionalScore > simplifiedScore) "zh-Hant" else "zh-CN"
}

private fun StringBuilder.appendLineJson(
    line: ISyncedLine,
    sourceIndex: Int,
    clusterIndex: Int,
    clusterRole: String
) {
    when (line) {
        is KaraokeLine -> appendKaraokeLine(line, sourceIndex, clusterIndex, clusterRole)
        is SyncedLine -> appendSyncedLine(line, sourceIndex, clusterIndex, clusterRole)
        else -> appendSyncedLine(
            sourceIndex = sourceIndex,
            clusterIndex = clusterIndex,
            clusterRole = clusterRole,
            start = line.start,
            end = line.end,
            content = "",
            translation = null
        )
    }
}

private fun StringBuilder.appendKaraokeLine(
    line: KaraokeLine,
    sourceIndex: Int,
    clusterIndex: Int,
    clusterRole: String
) {
    append('{')
    appendJsonField("kind", "karaoke")
    append(',')
    appendJsonField("source_index", sourceIndex)
    append(',')
    appendJsonField("cluster_index", clusterIndex)
    append(',')
    appendJsonField("cluster_role", clusterRole)
    append(',')
    appendJsonField("start", line.start)
    append(',')
    appendJsonField("end", line.end)
    append(',')
    appendJsonField("is_accompaniment", line is KaraokeLine.AccompanimentKaraokeLine)
    append(',')
    appendJsonField("alignment", line.alignment.toRendererValue())
    append(',')
    appendJsonNullableField("translation", line.translation)
    append(',')
    appendJsonNullableField("phonetic", line.phonetic)
    append(',')
    append("\"syllables\":[")
    line.syllables.forEachIndexed { index, syllable ->
        if (index > 0) append(',')
        append('{')
        appendJsonField("content", syllable.content)
        append(',')
        appendJsonField("start", syllable.start)
        append(',')
        appendJsonField("end", syllable.end)
        append(',')
        appendJsonNullableField("phonetic", syllable.phonetic)
        append('}')
    }
    append(']')
    append('}')
}

private fun StringBuilder.appendSyncedLine(
    line: SyncedLine,
    sourceIndex: Int,
    clusterIndex: Int,
    clusterRole: String
) {
    appendSyncedLine(
        sourceIndex = sourceIndex,
        clusterIndex = clusterIndex,
        clusterRole = clusterRole,
        start = line.start,
        end = line.end,
        content = line.content,
        translation = line.translation
    )
}

private fun StringBuilder.appendSyncedLine(
    sourceIndex: Int,
    clusterIndex: Int,
    clusterRole: String,
    start: Int,
    end: Int,
    content: String,
    translation: String?
) {
    append('{')
    appendJsonField("kind", "synced")
    append(',')
    appendJsonField("source_index", sourceIndex)
    append(',')
    appendJsonField("cluster_index", clusterIndex)
    append(',')
    appendJsonField("cluster_role", clusterRole)
    append(',')
    appendJsonField("start", start)
    append(',')
    appendJsonField("end", end)
    append(',')
    appendJsonField("content", content)
    append(',')
    appendJsonNullableField("translation", translation)
    append('}')
}

private fun KaraokeAlignment.toRendererValue(): String {
    return when (this) {
        KaraokeAlignment.Start -> "start"
        KaraokeAlignment.End -> "end"
        KaraokeAlignment.Unspecified -> "unspecified"
    }
}

private fun StringBuilder.appendJsonField(name: String, value: String) {
    append('"').append(name).append("\":")
    appendJsonString(value)
}

private fun StringBuilder.appendJsonNullableField(name: String, value: String?) {
    append('"').append(name).append("\":")
    if (value == null) append("null") else appendJsonString(value)
}

private fun StringBuilder.appendJsonField(name: String, value: Int) {
    append('"').append(name).append("\":").append(value)
}

private fun StringBuilder.appendJsonField(name: String, value: Long) {
    append('"').append(name).append("\":").append(value)
}

private fun StringBuilder.appendJsonField(name: String, value: Float) {
    append('"').append(name).append("\":").append(value)
}

private fun StringBuilder.appendJsonField(name: String, value: Boolean) {
    append('"').append(name).append("\":").append(value)
}

private fun StringBuilder.appendJsonString(value: String) {
    append('"')
    value.forEach { char ->
        when (char) {
            '\\' -> append("\\\\")
            '"' -> append("\\\"")
            '\b' -> append("\\b")
            '\u000C' -> append("\\f")
            '\n' -> append("\\n")
            '\r' -> append("\\r")
            '\t' -> append("\\t")
            else -> {
                if (char.code < 0x20) {
                    append("\\u")
                    append(char.code.toString(16).padStart(4, '0'))
                } else {
                    append(char)
                }
            }
        }
    }
    append('"')
}
