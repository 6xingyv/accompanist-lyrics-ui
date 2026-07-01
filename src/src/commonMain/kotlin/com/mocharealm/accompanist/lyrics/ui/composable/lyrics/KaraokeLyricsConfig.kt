package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnitType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.mocharealm.accompanist.lyrics.ui.renderer.NativeBlurStyle
import com.mocharealm.accompanist.lyrics.ui.renderer.NativeBreathingDotsStyle
import com.mocharealm.accompanist.lyrics.ui.renderer.NativeFocusStyle
import com.mocharealm.accompanist.lyrics.ui.renderer.NativeLyricsRendererStyle
import com.mocharealm.accompanist.lyrics.ui.renderer.NativeManualScrollStyle
import com.mocharealm.accompanist.lyrics.ui.renderer.NativeSpacingStyle
import com.mocharealm.accompanist.lyrics.ui.renderer.NativeSpringStyle
import com.mocharealm.accompanist.lyrics.ui.renderer.NativeTypographyStyle

/**
 * Single source of truth for every tunable of the native karaoke renderer.
 *
 * Previously these lived scattered across the [KaraokeLyricsView] parameter list,
 * two `defaultStyle()` functions and both platform host builders (with conflicting
 * values), while several knobs — the per-line scroll spring, manual-scroll physics,
 * blur sharp band and focus dimming — were only reachable as Rust constants. They
 * are now grouped here, each with a default that reproduces the previous look
 * exactly, so a single `KaraokeLyricsConfig()` renders identically to before.
 */
data class KaraokeLyricsConfig(
    val typography: KaraokeTypography = KaraokeTypography(),
    val spacing: KaraokeSpacing = KaraokeSpacing(),
    val blur: KaraokeBlurConfig = KaraokeBlurConfig(),
    val focus: KaraokeFocusConfig = KaraokeFocusConfig(),
    val autoScrollSpring: KaraokeSpringConfig = KaraokeSpringConfig(),
    val manualScroll: KaraokeManualScrollConfig = KaraokeManualScrollConfig(),
    val breathingDots: KaraokeBreathingDotsConfigs = KaraokeBreathingDotsConfigs(),
    val textColor: Color = Color.White,
    val showTranslation: Boolean = true,
    val showPhonetic: Boolean = true,
    /** Compose-side blend mode for the whole view; not forwarded to the renderer. */
    val blendMode: BlendMode = BlendMode.Plus,
    val showDebugRectangles: Boolean = false,
)

/**
 * Per-role type. Every role — including the translation — is a full [TextStyle],
 * so its font size, weight, italic **and line height** are specified in one place.
 * (The translation used to be derived as a fraction of the normal font, `0.46/
 * 0.62`, and could not be styled or sized independently.) Line height is taken
 * from each style's [TextStyle.lineHeight]; when a style leaves it unspecified the
 * matching `*LineHeightRatio` fallback is applied (× the font size).
 */
data class KaraokeTypography(
    val normalTextStyle: TextStyle = TextStyle(
        fontSize = 34.sp,
        fontWeight = FontWeight.Bold,
        lineHeight = 40.sp,
    ),
    val accompanimentTextStyle: TextStyle = TextStyle(
        fontSize = 20.sp,
        fontWeight = FontWeight.Bold,
        lineHeight = 26.sp,
    ),
    val translationTextStyle: TextStyle = TextStyle(
        fontSize = 16.sp,
        fontWeight = FontWeight.Normal,
        lineHeight = 18.sp,
    ),
    val phoneticTextStyle: TextStyle = TextStyle(
        fontSize = 24.sp,
        fontWeight = FontWeight.Normal,
        lineHeight = 30.sp,
    ),
    /** Line-height fallback ratios (× font size), used only for a role whose
     * [TextStyle.lineHeight] is [TextUnit.Unspecified]. */
    val normalLineHeightRatio: Float = 1.25f,
    val accompanimentLineHeightRatio: Float = 1.3f,
    val translationLineHeightRatio: Float = 1.3f,
    val phoneticLineHeightRatio: Float = 1.25f,
)

/**
 * Vertical/horizontal spacing. `linePadding` is the per-line vertical padding, so
 * the gap between two separate lines is `2 * linePadding`. `accompanimentGap` is
 * the gap between a main line and its own nested accompaniment line; it *replaces*
 * the `linePadding`-derived gap for that boundary (it is not added on top), so the
 * harmony line can sit tighter than normal lines. `focusTopOffset` is where the
 * focused line parks from the top (was `offset + keepAliveZone`).
 */
data class KaraokeSpacing(
    val horizontalPadding: Dp = 16.dp,
    val linePadding: Dp = 12.dp,
    val accompanimentGap: Dp = 8.dp,
    val phoneticGap: Dp = 4.dp,
    val focusTopOffset: Dp = 64.dp,
)

/** Depth-of-field blur. `sharpRadiusLines` is the fully-sharp band around the focus. */
data class KaraokeBlurConfig(
    val enabled: Boolean = true,
    val delta: Float = 1f,
    val sharpRadiusLines: Float = 2.5f,
)

/** How far-from-focus lines dim, and how unsung karaoke syllables are greyed. */
data class KaraokeFocusConfig(
    val inactiveKaraokeAlpha: Float = 0.2f,
    val dimMinAlpha: Float = 0.4f,
    val dimFalloffMs: Int = 400,
)

/** The per-line auto-scroll spring cascade. */
data class KaraokeSpringConfig(
    val stiffness: Float = 80f,
    val damping: Float = 12f,
    val chainCoupling: Float = 0.65f,
    val distanceFalloff: Float = 0.25f,
    val minResponse: Float = 0.35f,
)

/** Touch fling / overscroll physics and the blur-restore timing after a manual scroll. */
data class KaraokeManualScrollConfig(
    val maxFlingVelocity: Float = 14000f,
    val decelerationRate: Float = 0.998f,
    val overscrollStiffness: Float = 119.4f,
    val overscrollDamping: Float = 21.85f,
    val rubberBandLimit: Float = 180f,
    val rubberBandCoefficient: Float = 0.55f,
    val blurRestoreMs: Int = 2500,
    val blurFadeInRate: Float = 6f,
    val blurFadeOutRate: Float = 12f,
)

/**
 * Flattens the grouped config into the px-based wire struct handed to the Rust
 * engine. This is the single place density conversion happens — it replaces the
 * duplicated `NativeLyricsRendererStyle(...)` blocks in the platform host builders.
 */
internal fun KaraokeLyricsConfig.toRendererStyle(density: Density): NativeLyricsRendererStyle =
    with(density) {
        NativeLyricsRendererStyle(
            typography = NativeTypographyStyle(
                normalFontSizePx = typography.normalTextStyle.fontSize.toPx(),
                normalLineHeightPx = lineHeightPx(
                    typography.normalTextStyle, typography.normalLineHeightRatio
                ),
                normalFontWeight = typography.normalTextStyle.fontWeight?.weight ?: 400,
                normalFontItalic = typography.normalTextStyle.fontStyle == FontStyle.Italic,
                accompanimentFontSizePx = typography.accompanimentTextStyle.fontSize.toPx(),
                accompanimentLineHeightPx = lineHeightPx(
                    typography.accompanimentTextStyle, typography.accompanimentLineHeightRatio
                ),
                accompanimentFontWeight = typography.accompanimentTextStyle.fontWeight?.weight ?: 400,
                accompanimentFontItalic = typography.accompanimentTextStyle.fontStyle == FontStyle.Italic,
                translationFontSizePx = typography.translationTextStyle.fontSize.toPx(),
                translationLineHeightPx = lineHeightPx(
                    typography.translationTextStyle, typography.translationLineHeightRatio
                ),
                translationFontWeight = typography.translationTextStyle.fontWeight?.weight ?: 400,
                translationFontItalic = typography.translationTextStyle.fontStyle == FontStyle.Italic,
                phoneticFontSizePx = typography.phoneticTextStyle.fontSize.toPx(),
                phoneticLineHeightPx = lineHeightPx(
                    typography.phoneticTextStyle, typography.phoneticLineHeightRatio
                ),
                phoneticFontWeight = typography.phoneticTextStyle.fontWeight?.weight ?: 400,
                phoneticFontItalic = typography.phoneticTextStyle.fontStyle == FontStyle.Italic,
            ),
            spacing = NativeSpacingStyle(
                paddingXPx = spacing.horizontalPadding.toPx(),
                paddingYPx = spacing.linePadding.toPx(),
                phoneticGapPx = spacing.phoneticGap.toPx(),
                accompanimentGapPx = spacing.accompanimentGap.toPx(),
                keepAlivePx = spacing.focusTopOffset.toPx(),
            ),
            blur = NativeBlurStyle(
                useBlurEffect = blur.enabled,
                blurDelta = blur.delta,
                sharpRadiusLines = blur.sharpRadiusLines,
            ),
            focus = NativeFocusStyle(
                inactiveKaraokeAlpha = focus.inactiveKaraokeAlpha,
                dimMinAlpha = focus.dimMinAlpha,
                dimFalloffMs = focus.dimFalloffMs,
            ),
            spring = NativeSpringStyle(
                stiffness = autoScrollSpring.stiffness,
                damping = autoScrollSpring.damping,
                chainCoupling = autoScrollSpring.chainCoupling,
                distanceFalloff = autoScrollSpring.distanceFalloff,
                minResponse = autoScrollSpring.minResponse,
            ),
            manualScroll = NativeManualScrollStyle(
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
            breathingDots = NativeBreathingDotsStyle(
                number = breathingDots.number,
                sizePx = breathingDots.size.toPx(),
                marginPx = breathingDots.margin.toPx(),
                enterMs = breathingDots.enterDurationMs,
                stillMs = breathingDots.preExitStillDuration,
                dipMs = breathingDots.preExitDipAndRiseDuration,
                exitMs = breathingDots.exitDurationMs,
                colorArgb = breathingDots.breathingDotsColor.toArgb(),
            ),
            textColorArgb = textColor.toArgb(),
            showTranslation = showTranslation,
            showPhonetic = showPhonetic,
        )
    }

/**
 * Resolve a role's line height to px: prefer the explicit [TextStyle.lineHeight]
 * (sp, or em × font size), falling back to `fontSize * fallbackRatio` when it is
 * [TextUnit.Unspecified].
 */
private fun Density.lineHeightPx(style: TextStyle, fallbackRatio: Float): Float =
    when (style.lineHeight.type) {
        TextUnitType.Sp -> style.lineHeight.toPx()
        TextUnitType.Em -> style.fontSize.toPx() * style.lineHeight.value
        else -> style.fontSize.toPx() * fallbackRatio
    }
