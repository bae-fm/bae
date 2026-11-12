package fm.bae.app.ui.playback

import android.view.ViewGroup
import android.widget.SeekBar
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.ui.appearance.ThemeOpacity
import kotlin.math.roundToInt

private const val VOLUME_STEPS = 1_000

/**
 * The volume bar: the full player's seek line, drawn the same way, at [value] (0 to 1). It reports
 * the level under the finger as it moves, [onValueChange], and the level the drag ended on,
 * [onValueChangeFinished].
 */
@Composable
internal fun AndroidVolumeSlider(
    value: Float,
    onValueChange: (Float) -> Unit,
    onValueChangeFinished: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = MaterialTheme.colorScheme
    val onValueChangeNow by rememberUpdatedState(onValueChange)
    val onValueChangeFinishedNow by rememberUpdatedState(onValueChangeFinished)
    AndroidView(
        modifier = modifier.fillMaxWidth(),
        factory = { context ->
            val density = context.resources.displayMetrics.density
            val look = SeekLineLook.LARGE
            SeekBar(context).apply {
                max = VOLUME_STEPS
                thumb = null
                progressDrawable = SeekLineDrawable(density, look)
                val thumbHalf = Math.ceil((look.thumbHalfWidthDp * density).toDouble()).toInt()
                setPaddingRelative(thumbHalf, 0, thumbHalf, 0)
                layoutParams =
                    ViewGroup.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        (look.touchHeightDp * density).roundToInt(),
                    )
                setOnSeekBarChangeListener(
                    object : SeekBar.OnSeekBarChangeListener {
                        override fun onProgressChanged(
                            seekBar: SeekBar,
                            progress: Int,
                            fromUser: Boolean,
                        ) {
                            if (fromUser) onValueChangeNow(progress.toFloat() / VOLUME_STEPS)
                        }

                        override fun onStartTrackingTouch(seekBar: SeekBar) = Unit

                        override fun onStopTrackingTouch(seekBar: SeekBar) = onValueChangeFinishedNow()
                    },
                )
            }
        },
        update = { bar ->
            val line = bar.progressDrawable as SeekLineDrawable
            line.accent = colors.primary.toArgb()
            line.track = colors.primary.copy(alpha = ThemeOpacity.tintStrong).toArgb()
            val level = (value.coerceIn(0f, 1f) * VOLUME_STEPS).roundToInt()
            if (bar.progress != level) bar.progress = level
        },
    )
}
