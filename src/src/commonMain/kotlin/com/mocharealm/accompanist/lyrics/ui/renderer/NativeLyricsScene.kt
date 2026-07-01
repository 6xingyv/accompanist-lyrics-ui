package com.mocharealm.accompanist.lyrics.ui.renderer

import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine

/**
 * Flat, px-based render style handed to the Rust engine. Grouped into sub-styles
 * that mirror [com.mocharealm.accompanist.lyrics.ui.composable.lyrics.KaraokeLyricsConfig];
 * built only by that config's `toRendererStyle` mapper. The JSON wire keys are
 * still flat (see `toNativeLyricsSceneJson`), so the engine side is unaffected.
 */
data class NativeLyricsRendererStyle(
    val typography: NativeTypographyStyle,
    val spacing: NativeSpacingStyle,
    val blur: NativeBlurStyle,
    val focus: NativeFocusStyle,
    val spring: NativeSpringStyle,
    val manualScroll: NativeManualScrollStyle,
    val breathingDots: NativeBreathingDotsStyle,
    val textColorArgb: Int,
    val showTranslation: Boolean = true,
    val showPhonetic: Boolean = true,
)

/** Per-role font size / line height (px) / weight / italic. */
data class NativeTypographyStyle(
    val normalFontSizePx: Float,
    val normalLineHeightPx: Float,
    val normalFontWeight: Int,
    val normalFontItalic: Boolean,
    val accompanimentFontSizePx: Float,
    val accompanimentLineHeightPx: Float,
    val accompanimentFontWeight: Int,
    val accompanimentFontItalic: Boolean,
    val translationFontSizePx: Float,
    val translationLineHeightPx: Float,
    val translationFontWeight: Int,
    val translationFontItalic: Boolean,
    val phoneticFontSizePx: Float,
    val phoneticLineHeightPx: Float,
    val phoneticFontWeight: Int,
    val phoneticFontItalic: Boolean,
)

/** Spacing (px). `accompanimentGapPx` is additive between a main line and its accompaniment. */
data class NativeSpacingStyle(
    val paddingXPx: Float,
    val paddingYPx: Float,
    val phoneticGapPx: Float,
    val accompanimentGapPx: Float,
    val keepAlivePx: Float,
)

/** Depth-of-field blur. `sharpRadiusLines` is in line-height units. */
data class NativeBlurStyle(
    val useBlurEffect: Boolean,
    val blurDelta: Float,
    val sharpRadiusLines: Float,
)

/** Focus dimming / inactive karaoke syllable alpha. */
data class NativeFocusStyle(
    val inactiveKaraokeAlpha: Float,
    val dimMinAlpha: Float,
    val dimFalloffMs: Int,
)

/** Per-line auto-scroll spring cascade. */
data class NativeSpringStyle(
    val stiffness: Float,
    val damping: Float,
    val chainCoupling: Float,
    val distanceFalloff: Float,
    val minResponse: Float,
)

/** Manual (touch) scroll physics. */
data class NativeManualScrollStyle(
    val maxFlingVelocity: Float,
    val decelerationRate: Float,
    val overscrollStiffness: Float,
    val overscrollDamping: Float,
    val rubberBandLimit: Float,
    val rubberBandCoefficient: Float,
    val blurRestoreMs: Int,
    val blurFadeInRate: Float,
    val blurFadeOutRate: Float,
)

/** Interlude breathing-dots geometry / timing. */
data class NativeBreathingDotsStyle(
    val number: Int,
    val sizePx: Float,
    val marginPx: Float,
    val enterMs: Int,
    val stillMs: Int,
    val dipMs: Int,
    val exitMs: Int,
    val colorArgb: Int,
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
        val typography = style.typography
        val spacing = style.spacing
        val blur = style.blur
        val focus = style.focus
        val spring = style.spring
        val manual = style.manualScroll
        val dots = style.breathingDots
        appendJsonField("normal_font_size", typography.normalFontSizePx)
        append(',')
        appendJsonField("normal_line_height", typography.normalLineHeightPx)
        append(',')
        appendJsonField("normal_font_weight", typography.normalFontWeight)
        append(',')
        appendJsonField("normal_font_italic", typography.normalFontItalic)
        append(',')
        appendJsonField("accompaniment_font_size", typography.accompanimentFontSizePx)
        append(',')
        appendJsonField("accompaniment_line_height", typography.accompanimentLineHeightPx)
        append(',')
        appendJsonField("accompaniment_font_weight", typography.accompanimentFontWeight)
        append(',')
        appendJsonField("accompaniment_font_italic", typography.accompanimentFontItalic)
        append(',')
        appendJsonField("translation_font_size", typography.translationFontSizePx)
        append(',')
        appendJsonField("translation_line_height", typography.translationLineHeightPx)
        append(',')
        appendJsonField("translation_font_weight", typography.translationFontWeight)
        append(',')
        appendJsonField("translation_font_italic", typography.translationFontItalic)
        append(',')
        appendJsonField("phonetic_font_size", typography.phoneticFontSizePx)
        append(',')
        appendJsonField("phonetic_line_height", typography.phoneticLineHeightPx)
        append(',')
        appendJsonField("phonetic_font_weight", typography.phoneticFontWeight)
        append(',')
        appendJsonField("phonetic_font_italic", typography.phoneticFontItalic)
        append(',')
        appendJsonField("phonetic_gap", spacing.phoneticGapPx)
        append(',')
        appendJsonField("padding_x", spacing.paddingXPx)
        append(',')
        appendJsonField("padding_y", spacing.paddingYPx)
        append(',')
        appendJsonField("keep_alive", spacing.keepAlivePx)
        append(',')
        appendJsonField("text_color", style.textColorArgb.toUInt().toLong())
        append(',')
        appendJsonField("show_translation", style.showTranslation)
        append(',')
        appendJsonField("show_phonetic", style.showPhonetic)
        append(',')
        appendJsonField("use_blur_effect", blur.useBlurEffect)
        append(',')
        appendJsonField("blur_delta", blur.blurDelta)
        append(',')
        appendJsonField("breathing_dots_number", dots.number)
        append(',')
        appendJsonField("breathing_dots_size", dots.sizePx)
        append(',')
        appendJsonField("breathing_dots_margin", dots.marginPx)
        append(',')
        appendJsonField("breathing_dots_enter_ms", dots.enterMs)
        append(',')
        appendJsonField("breathing_dots_still_ms", dots.stillMs)
        append(',')
        appendJsonField("breathing_dots_dip_ms", dots.dipMs)
        append(',')
        appendJsonField("breathing_dots_exit_ms", dots.exitMs)
        append(',')
        appendJsonField("breathing_dots_color", dots.colorArgb.toUInt().toLong())
        append(',')
        appendJsonField("accompaniment_gap", spacing.accompanimentGapPx)
        append(',')
        appendJsonField("blur_sharp_radius_lines", blur.sharpRadiusLines)
        append(',')
        appendJsonField("inactive_karaoke_alpha", focus.inactiveKaraokeAlpha)
        append(',')
        appendJsonField("focus_dim_min_alpha", focus.dimMinAlpha)
        append(',')
        appendJsonField("focus_dim_falloff_ms", focus.dimFalloffMs)
        append(',')
        appendJsonField("spring_stiffness", spring.stiffness)
        append(',')
        appendJsonField("spring_damping", spring.damping)
        append(',')
        appendJsonField("spring_chain_coupling", spring.chainCoupling)
        append(',')
        appendJsonField("spring_distance_falloff", spring.distanceFalloff)
        append(',')
        appendJsonField("spring_min_response", spring.minResponse)
        append(',')
        appendJsonField("manual_max_fling_velocity", manual.maxFlingVelocity)
        append(',')
        appendJsonField("manual_deceleration_rate", manual.decelerationRate)
        append(',')
        appendJsonField("manual_overscroll_stiffness", manual.overscrollStiffness)
        append(',')
        appendJsonField("manual_overscroll_damping", manual.overscrollDamping)
        append(',')
        appendJsonField("manual_rubber_band_limit", manual.rubberBandLimit)
        append(',')
        appendJsonField("manual_rubber_band_coefficient", manual.rubberBandCoefficient)
        append(',')
        appendJsonField("manual_blur_restore_ms", manual.blurRestoreMs)
        append(',')
        appendJsonField("manual_blur_fade_in_rate", manual.blurFadeInRate)
        append(',')
        appendJsonField("manual_blur_fade_out_rate", manual.blurFadeOutRate)
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
