package fm.bae.app.ui.components

import android.provider.Settings
import androidx.compose.animation.core.withInfiniteAnimationFrameMillis
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.times
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewTheme
import fm.bae.app.ui.appearance.SemanticColors
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sqrt

/**
 * Every proportion is a fraction of the sheep's width, as the README's Logo
 * section (`design/system/README.md`) gives them.
 */
private object Proportion {
    const val SHEEP_HEIGHT = 545f / 584f
    const val WORD_WIDTH = 0.52f
    const val WORD_ASPECT = 1556f / 736f
    const val GAP = 0.06f
    const val GLOW_WIDTH = 2.6f
    const val GLOW_CENTRE = 0.65f
    const val POOL_WIDTH = 1.25f
    const val POOL_HEIGHT = 0.34f
    const val POOL_CENTRE = 0.90f

    /** Where the pool's gradient ends, as a fraction of its width: the ellipse's far corner. */
    val POOL_REACH = 0.5f * sqrt(2f)
    const val GLOW_INNER_STOP = 0.30f
    const val GLOW_MIDDLE_STOP = 0.55f
    const val GLOW_OUTER_STOP = 0.80f
    const val POOL_MIDDLE_STOP = 0.5f
    const val POOL_CLEAR_STOP = 0.72f
    const val SHEEP_RISE = 0.04f
    const val WORD_RISE = 0.15f
}

private object Motion {
    const val RISE_SECONDS = 6.0
    const val TILT_SECONDS = 7.3
    const val TILT_DEGREES = 2.0
    const val WORD_DELAY_SECONDS = 2.0
}

private val wordEdgeOffset = 1.dp

/**
 * The sheep over the word "bae". [size] is the sheep's width; the word and the
 * glow scale with it. With [bob] the sheep and the word each float and tilt,
 * unless the system's animations are off; with [glow] a soft disc of light sits
 * behind the sheep and a pool of light under it, both still.
 */
@Composable
internal fun Logo(
    size: Dp,
    bob: Boolean,
    glow: Boolean,
    modifier: Modifier = Modifier,
) {
    val colors = BaeTheme.colors
    val sheepHeight = size * Proportion.SHEEP_HEIGHT
    val wordWidth = size * Proportion.WORD_WIDTH
    val wordHeight = wordWidth / Proportion.WORD_ASPECT
    val wordRise = Proportion.WORD_RISE * wordHeight
    val seconds = rememberBobSeconds(bob)
    Column(
        modifier = modifier.semantics(mergeDescendants = true) { contentDescription = "bae" },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(size * Proportion.GAP),
    ) {
        // The sheep's slot is the glow's anchor, so the glow stays still while the sheep floats over it.
        Box(
            modifier =
                Modifier.size(size, sheepHeight).drawBehind {
                    if (glow) {
                        drawGlow(colors)
                    }
                },
        ) {
            Image(
                painter = painterResource(R.drawable.logo_sheep),
                contentDescription = null,
                modifier = Modifier.size(size, sheepHeight).bobbing(seconds, Proportion.SHEEP_RISE * sheepHeight, 0.0),
            )
        }
        Box(
            modifier = Modifier.size(wordWidth, wordHeight).bobbing(seconds, wordRise, Motion.WORD_DELAY_SECONDS),
        ) {
            Image(
                painter = painterResource(R.drawable.logo_word),
                contentDescription = null,
                colorFilter = ColorFilter.tint(colors.logoWordEdge),
                modifier = Modifier.size(wordWidth, wordHeight).offset(wordEdgeOffset, wordEdgeOffset),
            )
            Image(
                painter = painterResource(R.drawable.logo_word),
                contentDescription = null,
                colorFilter = ColorFilter.tint(colors.logoWord),
                modifier = Modifier.size(wordWidth, wordHeight),
            )
        }
    }
}

/**
 * The seconds since the logo appeared, read only inside a graphics layer so a
 * frame redraws without recomposing; null when it doesn't move: no [bob], the
 * system's animations off ("Remove animations").
 */
@Composable
private fun rememberBobSeconds(bob: Boolean): (() -> Double)? {
    val resolver = LocalContext.current.contentResolver
    val moves = bob && Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) != 0f
    var millis by remember { mutableLongStateOf(0L) }
    LaunchedEffect(moves) {
        if (!moves) return@LaunchedEffect
        val start = withInfiniteAnimationFrameMillis { it }
        while (true) {
            withInfiniteAnimationFrameMillis { millis = it - start }
        }
    }
    return if (moves) ({ millis / MILLIS_PER_SECOND }) else null
}

private const val MILLIS_PER_SECOND = 1000.0

/**
 * Rises by [rise] pixels and back every 6 seconds and tilts 2° each way every
 * 7.3, [delay] seconds behind; still when [seconds] is null.
 */
private fun Modifier.bobbing(
    seconds: (() -> Double)?,
    rise: Dp,
    delay: Double,
): Modifier =
    if (seconds == null) {
        this
    } else {
        graphicsLayer {
            val time = seconds() - delay
            translationY = (-rise.toPx() * (1 - cos(phase(time, Motion.RISE_SECONDS))) / 2).toFloat()
            rotationZ = (-Motion.TILT_DEGREES * cos(phase(time, Motion.TILT_SECONDS))).toFloat()
        }
    }

private fun phase(
    time: Double,
    period: Double,
): Double = 2 * PI * time / period

/** The disc of light centred 65% down the sheep and the pool under it; neither takes layout. */
private fun DrawScope.drawGlow(colors: SemanticColors) {
    val discRadius = size.width * Proportion.GLOW_WIDTH / 2
    val disc = Offset(size.width / 2, size.height * Proportion.GLOW_CENTRE)
    drawCircle(
        brush =
            Brush.radialGradient(
                0f to colors.logoGlowCentre,
                Proportion.GLOW_INNER_STOP to colors.logoGlowInner,
                Proportion.GLOW_MIDDLE_STOP to colors.logoGlowMiddle,
                Proportion.GLOW_OUTER_STOP to colors.logoGlowOuter,
                1f to colors.logoGlowOuter.copy(alpha = 0f),
                center = disc,
                radius = discRadius,
            ),
        radius = discRadius,
        center = disc,
    )
    val poolWidth = size.width * Proportion.POOL_WIDTH
    val poolHeight = size.height * Proportion.POOL_HEIGHT
    // A circle of the pool's width squeezed to its height, so its gradient squeezes with it.
    withTransform({
        translate(size.width / 2, size.height * Proportion.POOL_CENTRE)
        scale(1f, poolHeight / poolWidth, pivot = Offset.Zero)
    }) {
        drawCircle(
            brush =
                Brush.radialGradient(
                    0f to colors.logoPoolCentre,
                    Proportion.POOL_MIDDLE_STOP to colors.logoPoolMiddle,
                    Proportion.POOL_CLEAR_STOP to colors.logoPoolMiddle.copy(alpha = 0f),
                    center = Offset.Zero,
                    radius = poolWidth * Proportion.POOL_REACH,
                ),
            radius = poolWidth / 2,
            center = Offset.Zero,
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun LogoPreview() {
    PreviewTheme {
        Box(modifier = Modifier.size(300.dp), contentAlignment = Alignment.Center) {
            Logo(size = 120.dp, bob = true, glow = true)
        }
    }
}
