package com.mocharealm.accompanist.lyrics.ui.renderer

import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.TextUnitType
import com.mocharealm.accompanist.lyrics.core.model.ISyncedLine
import com.mocharealm.accompanist.lyrics.core.model.SyncedLyrics
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeAlignment
import com.mocharealm.accompanist.lyrics.core.model.karaoke.KaraokeLine
import com.mocharealm.accompanist.lyrics.core.model.synced.SyncedLine
import com.mocharealm.accompanist.lyrics.ui.composable.lyrics.KaraokeLyricsConfig
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/**
 * The wire contract handed to the Rust engine. This is the single, px-based,
 * `kotlinx.serialization`-driven representation of a render scene — it replaces
 * the old hand-rolled JSON builder and the duplicate `NativeLyricsRendererStyle`
 * mirror of [KaraokeLyricsConfig].
 *
 * Field names here are the JSON keys and are kept identical to the engine's Rust
 * `serde` structs (see `text_engine/src/renderer.rs`), so the two sides share one
 * vocabulary. The presentation-side [KaraokeLyricsConfig] maps into it via
 * [toSceneStyle] (the only place density conversion happens); the data layer never
 * leaks back into the config.
 */
@Serializable
internal data class SceneStyle(
    val typography: TypographyStyle,
    val spacing: SpacingStyle,
    val blur: BlurStyle,
    val focus: FocusStyle,
    val autoScrollSpring: SpringStyle,
    val manualScroll: ManualScrollStyle,
    val breathingDots: BreathingDotsStyle,
    val textColor: Long,
    val showTranslation: Boolean,
    val showPhonetic: Boolean,
)

/** Per-role font size / line height (px) / weight / italic. */
@Serializable
internal data class TypographyStyle(
    val normalFontSize: Float,
    val normalLineHeight: Float,
    val normalFontWeight: Int,
    val normalFontItalic: Boolean,
    val accompanimentFontSize: Float,
    val accompanimentLineHeight: Float,
    val accompanimentFontWeight: Int,
    val accompanimentFontItalic: Boolean,
    val translationFontSize: Float,
    val translationLineHeight: Float,
    val translationFontWeight: Int,
    val translationFontItalic: Boolean,
    val accompanimentTranslationFontSize: Float,
    val accompanimentTranslationLineHeight: Float,
    val accompanimentTranslationFontWeight: Int,
    val accompanimentTranslationFontItalic: Boolean,
    val phoneticFontSize: Float,
    val phoneticLineHeight: Float,
    val phoneticFontWeight: Int,
    val phoneticFontItalic: Boolean,
)

/** Spacing (px). */
@Serializable
internal data class SpacingStyle(
    val horizontalPadding: Float,
    val linePadding: Float,
    val accompanimentGap: Float,
    val phoneticGap: Float,
    val focusTopOffset: Float,
    val translationGap: Float,
    val accompanimentTranslationGap: Float,
)

/** Depth-of-field blur. `sharpRadiusLines` is in line-height units. */
@Serializable
internal data class BlurStyle(
    val enabled: Boolean,
    val delta: Float,
    val sharpRadiusLines: Float,
)

/** Focus dimming / inactive karaoke syllable alpha. */
@Serializable
internal data class FocusStyle(
    val inactiveKaraokeAlpha: Float,
    val dimMinAlpha: Float,
    val dimFalloffMs: Int,
)

/** Per-line auto-scroll spring cascade. */
@Serializable
internal data class SpringStyle(
    val stiffness: Float,
    val damping: Float,
    val chainCoupling: Float,
    val distanceFalloff: Float,
    val minResponse: Float,
)

/** Manual (touch) scroll physics. */
@Serializable
internal data class ManualScrollStyle(
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
@Serializable
internal data class BreathingDotsStyle(
    val number: Int,
    val size: Float,
    val margin: Float,
    val enterMs: Int,
    val stillMs: Int,
    val dipMs: Int,
    val exitMs: Int,
    val color: Long,
)

/** A full scene: viewport, locale, resolved style, and the flattened line list.
 * `contentTop`/`contentBottom` are the vertical content insets (px) for the lyrics
 * band when the engine owns the whole full-bleed surface (0 = fill, legacy). */
@Serializable
internal data class LyricsSceneWire(
    val width: Int,
    val height: Int,
    val locale: String,
    val contentTop: Float = 0f,
    val contentBottom: Float = 0f,
    val style: SceneStyle,
    val lines: List<LyricsLineWire>,
)

@Serializable
internal sealed interface LyricsLineWire {
    val sourceIndex: Int
    val clusterIndex: Int
    val clusterRole: String
    val start: Int
    val end: Int
}

@Serializable
@SerialName("karaoke")
internal data class KaraokeLineWire(
    override val sourceIndex: Int,
    override val clusterIndex: Int,
    override val clusterRole: String,
    override val start: Int,
    override val end: Int,
    val isAccompaniment: Boolean,
    val alignment: String,
    val translation: String?,
    val phonetic: String?,
    val syllables: List<SyllableWire>,
) : LyricsLineWire

@Serializable
@SerialName("synced")
internal data class SyncedLineWire(
    override val sourceIndex: Int,
    override val clusterIndex: Int,
    override val clusterRole: String,
    override val start: Int,
    override val end: Int,
    val content: String,
    val translation: String?,
) : LyricsLineWire

@Serializable
internal data class SyllableWire(
    val content: String,
    val start: Int,
    val end: Int,
    val phonetic: String?,
)

private val lyricsSceneJson = Json {
    classDiscriminator = "kind"
    encodeDefaults = true
}

/**
 * Flatten the config into the px-based [SceneStyle] wire. This is the single place
 * density conversion happens (it replaces the old `toRendererStyle`), and it lives
 * in the data layer so [KaraokeLyricsConfig] carries no dependency on the wire.
 */
internal fun KaraokeLyricsConfig.toSceneStyle(density: Density): SceneStyle =
    with(density) {
        SceneStyle(
            typography = TypographyStyle(
                normalFontSize = typography.normalTextStyle.fontSize.toPx(),
                normalLineHeight = lineHeightPx(
                    typography.normalTextStyle, typography.normalLineHeightRatio
                ),
                normalFontWeight = typography.normalTextStyle.fontWeight?.weight ?: 400,
                normalFontItalic = typography.normalTextStyle.fontStyle == FontStyle.Italic,
                accompanimentFontSize = typography.accompanimentTextStyle.fontSize.toPx(),
                accompanimentLineHeight = lineHeightPx(
                    typography.accompanimentTextStyle, typography.accompanimentLineHeightRatio
                ),
                accompanimentFontWeight = typography.accompanimentTextStyle.fontWeight?.weight ?: 400,
                accompanimentFontItalic = typography.accompanimentTextStyle.fontStyle == FontStyle.Italic,
                translationFontSize = typography.translationTextStyle.fontSize.toPx(),
                translationLineHeight = lineHeightPx(
                    typography.translationTextStyle, typography.translationLineHeightRatio
                ),
                translationFontWeight = typography.translationTextStyle.fontWeight?.weight ?: 400,
                translationFontItalic = typography.translationTextStyle.fontStyle == FontStyle.Italic,
                accompanimentTranslationFontSize = typography.accompanimentTranslationTextStyle.fontSize.toPx(),
                accompanimentTranslationLineHeight = lineHeightPx(
                    typography.accompanimentTranslationTextStyle,
                    typography.accompanimentTranslationLineHeightRatio
                ),
                accompanimentTranslationFontWeight = typography.accompanimentTranslationTextStyle.fontWeight?.weight ?: 400,
                accompanimentTranslationFontItalic = typography.accompanimentTranslationTextStyle.fontStyle == FontStyle.Italic,
                phoneticFontSize = typography.phoneticTextStyle.fontSize.toPx(),
                phoneticLineHeight = lineHeightPx(
                    typography.phoneticTextStyle, typography.phoneticLineHeightRatio
                ),
                phoneticFontWeight = typography.phoneticTextStyle.fontWeight?.weight ?: 400,
                phoneticFontItalic = typography.phoneticTextStyle.fontStyle == FontStyle.Italic,
            ),
            spacing = SpacingStyle(
                horizontalPadding = spacing.horizontalPadding.toPx(),
                linePadding = spacing.linePadding.toPx(),
                accompanimentGap = spacing.accompanimentGap.toPx(),
                phoneticGap = spacing.phoneticGap.toPx(),
                focusTopOffset = spacing.focusTopOffset.toPx(),
                translationGap = spacing.translationGap.toPx(),
                accompanimentTranslationGap = spacing.accompanimentTranslationGap.toPx(),
            ),
            blur = BlurStyle(
                enabled = blur.enabled,
                delta = blur.delta,
                sharpRadiusLines = blur.sharpRadiusLines,
            ),
            focus = FocusStyle(
                inactiveKaraokeAlpha = focus.inactiveKaraokeAlpha,
                dimMinAlpha = focus.dimMinAlpha,
                dimFalloffMs = focus.dimFalloffMs,
            ),
            autoScrollSpring = SpringStyle(
                stiffness = autoScrollSpring.stiffness,
                damping = autoScrollSpring.damping,
                chainCoupling = autoScrollSpring.chainCoupling,
                distanceFalloff = autoScrollSpring.distanceFalloff,
                minResponse = autoScrollSpring.minResponse,
            ),
            manualScroll = ManualScrollStyle(
                maxFlingVelocity = manualScroll.maxFlingVelocity,
                decelerationRate = manualScroll.decelerationRate,
                overscrollStiffness = manualScroll.overscrollStiffness,
                overscrollDamping = manualScroll.overscrollDamping,
                rubberBandLimit = manualScroll.rubberBandLimit,
                rubberBandCoefficient = manualScroll.rubberBandCoefficient,
                blurRestoreMs = manualScroll.blurRestoreMs,
                blurFadeInRate = manualScroll.blurFadeInRate,
                blurFadeOutRate = manualScroll.blurFadeOutRate,
            ),
            breathingDots = BreathingDotsStyle(
                number = breathingDots.number,
                size = breathingDots.size.toPx(),
                margin = breathingDots.margin.toPx(),
                enterMs = breathingDots.enterDurationMs,
                stillMs = breathingDots.preExitStillDuration,
                dipMs = breathingDots.preExitDipAndRiseDuration,
                exitMs = breathingDots.exitDurationMs,
                color = breathingDots.breathingDotsColor.toArgb().toUInt().toLong(),
            ),
            textColor = textColor.toArgb().toUInt().toLong(),
            showTranslation = showTranslation,
            showPhonetic = showPhonetic,
        )
    }

/**
 * Downscale only the px-spatial groups to match a downscaled render target. The
 * spring/manual-scroll physics and the unitless focus/blur ratios stay unscaled —
 * they match fixed engine constants that already live in the (downscaled) space.
 */
internal fun SceneStyle.scaled(scale: Float): SceneStyle {
    if (scale == 1f) return this
    return copy(
        typography = typography.copy(
            normalFontSize = typography.normalFontSize * scale,
            normalLineHeight = typography.normalLineHeight * scale,
            accompanimentFontSize = typography.accompanimentFontSize * scale,
            accompanimentLineHeight = typography.accompanimentLineHeight * scale,
            translationFontSize = typography.translationFontSize * scale,
            translationLineHeight = typography.translationLineHeight * scale,
            accompanimentTranslationFontSize = typography.accompanimentTranslationFontSize * scale,
            accompanimentTranslationLineHeight = typography.accompanimentTranslationLineHeight * scale,
            phoneticFontSize = typography.phoneticFontSize * scale,
            phoneticLineHeight = typography.phoneticLineHeight * scale,
        ),
        spacing = spacing.copy(
            horizontalPadding = spacing.horizontalPadding * scale,
            linePadding = spacing.linePadding * scale,
            accompanimentGap = spacing.accompanimentGap * scale,
            phoneticGap = spacing.phoneticGap * scale,
            focusTopOffset = spacing.focusTopOffset * scale,
            translationGap = spacing.translationGap * scale,
            accompanimentTranslationGap = spacing.accompanimentTranslationGap * scale,
        ),
        blur = blur.copy(delta = blur.delta * scale),
        breathingDots = breathingDots.copy(
            size = breathingDots.size * scale,
            margin = breathingDots.margin * scale,
        ),
    )
}

/** Serialize a scene (viewport + resolved [style] + lines) to the engine JSON.
 * `contentTop`/`contentBottom` are already in render px (downscale applied). */
internal fun SyncedLyrics.toSceneJson(
    width: Int,
    height: Int,
    style: SceneStyle,
    contentTop: Float = 0f,
    contentBottom: Float = 0f,
): String =
    lyricsSceneJson.encodeToString(
        LyricsSceneWire(
            width = width,
            height = height,
            locale = detectNativeLyricsLocale(),
            contentTop = contentTop,
            contentBottom = contentBottom,
            style = style,
            lines = toSceneLines(),
        )
    )

/**
 * Flatten the lyrics into the engine's line list, splitting a main line's nested
 * accompaniment into before/after roles around the main vocal. `start` is the `<p>`
 * begin, which a before-line accompaniment can pull earlier than the main vocal —
 * so it equals that accompaniment's own start; split on the first MAIN syllable
 * instead so a genuinely-earlier accompaniment stays a before-line one.
 */
private fun SyncedLyrics.toSceneLines(): List<LyricsLineWire> {
    val result = mutableListOf<LyricsLineWire>()
    lines.forEachIndexed { index, line ->
        if (line is KaraokeLine.MainKaraokeLine) {
            val accompaniment = line.accompanimentLines.orEmpty()
            val mainVocalStart = line.syllables.firstOrNull()?.start ?: line.start
            accompaniment
                .filter { it.start < mainVocalStart }
                .forEach { result += it.toWire(index, index, "before_accompaniment") }
            result += line.toWire(index, index, "main")
            accompaniment
                .filter { it.start >= mainVocalStart }
                .forEach { result += it.toWire(index, index, "after_accompaniment") }
        } else if (line !is KaraokeLine.AccompanimentKaraokeLine) {
            result += line.toWire(index, index, "standalone")
        }
    }
    return result
}

private fun ISyncedLine.toWire(
    sourceIndex: Int,
    clusterIndex: Int,
    clusterRole: String
): LyricsLineWire = when (this) {
    is KaraokeLine -> KaraokeLineWire(
        sourceIndex = sourceIndex,
        clusterIndex = clusterIndex,
        clusterRole = clusterRole,
        start = start,
        end = end,
        isAccompaniment = this is KaraokeLine.AccompanimentKaraokeLine,
        alignment = alignment.toWireValue(),
        translation = translation,
        phonetic = phonetic,
        syllables = syllables.map { SyllableWire(it.content, it.start, it.end, it.phonetic) },
    )

    is SyncedLine -> SyncedLineWire(
        sourceIndex = sourceIndex,
        clusterIndex = clusterIndex,
        clusterRole = clusterRole,
        start = start,
        end = end,
        content = content,
        translation = translation,
    )

    else -> SyncedLineWire(
        sourceIndex = sourceIndex,
        clusterIndex = clusterIndex,
        clusterRole = clusterRole,
        start = start,
        end = end,
        content = "",
        translation = null,
    )
}

private fun KaraokeAlignment.toWireValue(): String = when (this) {
    KaraokeAlignment.Start -> "start"
    KaraokeAlignment.End -> "end"
    KaraokeAlignment.Unspecified -> "unspecified"
}

/**
 * Resolve a role's line height to px: prefer the explicit [TextStyle.lineHeight]
 * (sp, or em × font size), falling back to `fontSize * fallbackRatio` when it is
 * unspecified.
 */
private fun Density.lineHeightPx(style: TextStyle, fallbackRatio: Float): Float =
    when (style.lineHeight.type) {
        TextUnitType.Sp -> style.lineHeight.toPx()
        TextUnitType.Em -> style.fontSize.toPx() * style.lineHeight.value
        else -> style.fontSize.toPx() * fallbackRatio
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
