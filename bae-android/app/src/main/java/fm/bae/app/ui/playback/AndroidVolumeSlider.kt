package fm.bae.app.ui.playback

import android.view.ViewGroup
import android.widget.SeekBar
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.VolumeOff
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.R
import fm.bae.app.data.receiverWithoutVolume
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.IconTone
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

private const val VOLUME_STEPS = 1_000

/**
 * The volume slider: the full player's seek line, drawn the same way, at [value] (0 to 1), a gap of
 * [ThemeSpace.related] to each side. It reports the level under the finger as it moves,
 * [onValueChange], and the level the drag ended on, [onValueChangeFinished].
 */
@Composable
internal fun AndroidVolumeSlider(
    value: Float,
    onValueChange: (Float) -> Unit,
    onValueChangeFinished: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = MaterialTheme.colorScheme
    val label = stringResource(R.string.volume)
    val onValueChangeNow by rememberUpdatedState(onValueChange)
    val onValueChangeFinishedNow by rememberUpdatedState(onValueChangeFinished)
    AndroidView(
        modifier = modifier.fillMaxWidth().padding(horizontal = ThemeSpace.related),
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
            bar.contentDescription = label
            val line = bar.progressDrawable as SeekLineDrawable
            line.accent = colors.primary.toArgb()
            line.track = colors.primary.copy(alpha = ThemeOpacity.tintStrong).toArgb()
            val level = (value.coerceIn(0f, 1f) * VOLUME_STEPS).roundToInt()
            if (bar.progress != level) bar.progress = level
        },
    )
}

/**
 * The full player's volume row: mute and the [AndroidVolumeSlider], or, casting to a receiver that
 * takes no volume from bae, the line saying where its volume is set.
 */
@Composable
internal fun AndroidVolumeRow(
    volume: Float,
    isMuted: Boolean,
    onSetMuted: (Boolean) -> Unit,
    onDragVolume: (Float) -> Unit,
    onSetVolume: suspend (Float) -> Unit,
    receiverWithoutVolume: String? = null,
) {
    // A receiver that takes no volume from bae keeps its volume itself: no mute
    // and no slider, which could only be refused, and a line saying where it is set.
    if (receiverWithoutVolume != null) {
        Box(modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp), contentAlignment = Alignment.Center) {
            Text(
                text = stringResource(R.string.volume_set_on_receiver, receiverWithoutVolume),
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
        }
        return
    }
    // Follow the finger from a local value while dragging, moving the output
    // as it goes. When the drag ends, the row calls onSetVolume once with the
    // last level and shows that level until the call returns; then the slider
    // shows [volume].
    var dragVolume by remember { mutableStateOf<Float?>(null) }
    val scope = rememberCoroutineScope()
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        AndroidIconButton(
            icon = if (isMuted) Icons.AutoMirrored.Filled.VolumeOff else Icons.AutoMirrored.Filled.VolumeUp,
            contentDescription = stringResource(if (isMuted) R.string.unmute else R.string.mute),
            onClick = { onSetMuted(!isMuted) },
            tone = IconTone.MUTED,
        )
        AndroidVolumeSlider(
            value = dragVolume ?: volume,
            onValueChange = {
                dragVolume = it
                onDragVolume(it)
            },
            onValueChangeFinished = {
                dragVolume?.let { level ->
                    scope.launch {
                        onSetVolume(level)
                        // When a new drag started while onSetVolume ran, keep that drag's level.
                        if (dragVolume == level) dragVolume = null
                    }
                }
            },
            modifier = Modifier.weight(1f),
        )
    }
}
