package com.mocharealm.accompanist.lyrics.ui.composable.lyrics

import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

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
        lineHeight = 20.sp,
    ),
    /** Translation shown under an accompaniment (background-vocal) line. */
    val accompanimentTranslationTextStyle: TextStyle = TextStyle(
        fontSize = 12.sp,
        fontWeight = FontWeight.Normal,
        lineHeight = 16.sp,
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
    val accompanimentTranslationLineHeightRatio: Float = 1.3f,
    val phoneticLineHeightRatio: Float = 1.25f,
)

/**
 * Vertical/horizontal spacing.
 */
data class KaraokeSpacing(
    // Matches the in-surface top bar's 28dp left margin / right button margin (see
    // RustSkiaLyricsView.resolveTopBar) so the lyrics text aligns with the top bar
    // on both edges.
    val horizontalPadding: Dp = 28.dp,
    val linePadding: Dp = 12.dp,
    val accompanimentGap: Dp = 8.dp,
    val phoneticGap: Dp = 8.dp,
    val focusTopOffset: Dp = 50.dp,
    /** Gap between a main line's body and its translation. */
    val translationGap: Dp = 8.dp,
    /** Gap between an accompaniment line's body and its translation. */
    val accompanimentTranslationGap: Dp = 4.dp,
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
    val dimMinAlpha: Float = 0.2f,
    val dimFalloffMs: Int = 400,
)

/** The per-line auto-scroll spring cascade. */
data class KaraokeSpringConfig(
    val stiffness: Float = 80f,
    val damping: Float = 14f,
    val chainCoupling: Float = 0.65f,
    val distanceFalloff: Float = 0.2f,
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
