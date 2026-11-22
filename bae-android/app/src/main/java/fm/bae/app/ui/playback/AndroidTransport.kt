package fm.bae.app.ui.playback

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material.icons.filled.SkipPrevious
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.ICON_BUTTON_BAR_BOX

/**
 * Previous, play and pause, and next. In the bar they sit side by side in 40dp boxes around a 24dp
 * play glyph; [full], in the full player, they are centred across the row, 56dp boxes around a 48dp
 * play glyph, with room between them.
 */
@Composable
internal fun AndroidTransport(
    isPlaying: Boolean,
    waitsOnStorage: Boolean,
    onPrevious: () -> Unit,
    onPlayPause: () -> Unit,
    onNext: () -> Unit,
    full: Boolean = false,
) {
    val skipGlyph = if (full) ThemeIcon.androidTransport else null
    val skipBox = if (full) FULL_SKIP_BOX else ICON_BUTTON_BAR_BOX
    Row(
        modifier = if (full) Modifier.fillMaxWidth() else Modifier,
        horizontalArrangement = if (full) Arrangement.Center else Arrangement.Start,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AndroidIconButton(
            icon = Icons.Filled.SkipPrevious,
            contentDescription = stringResource(R.string.player_previous),
            onClick = onPrevious,
            glyph = skipGlyph,
            box = skipBox,
        )
        if (full) Spacer(modifier = Modifier.width(ThemeSpace.section))
        AndroidPlayPause(
            isPlaying = isPlaying,
            iconSize = if (full) ThemeIcon.androidTransportPlay else ThemeIcon.large,
            waitsOnStorage = waitsOnStorage,
            onToggle = onPlayPause,
        )
        if (full) Spacer(modifier = Modifier.width(ThemeSpace.section))
        AndroidIconButton(
            icon = Icons.Filled.SkipNext,
            contentDescription = stringResource(R.string.player_next),
            onClick = onNext,
            glyph = skipGlyph,
            box = skipBox,
        )
    }
}

private val FULL_SKIP_BOX = 56.dp
