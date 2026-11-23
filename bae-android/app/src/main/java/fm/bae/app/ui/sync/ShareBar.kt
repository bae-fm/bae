package fm.bae.app.ui.sync

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.app.ui.components.AndroidProgressBarView
import fm.bae.fastvalues.LatestSource

/**
 * A bar that moves with no share to show, as the line for syncing has none: the accent over a
 * track of the accent's tint, drawn as [AndroidProgressBarView] draws a step that counts nothing. Screen
 * readers name it [description], the line's words.
 */
@Composable
internal fun MovingBar(
    description: String,
    modifier: Modifier = Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary
    val track = accent.copy(alpha = ThemeOpacity.tintStrong)
    AndroidView(
        modifier = modifier.fillMaxWidth(),
        factory = { context -> AndroidProgressBarView(context).apply { showMoving() } },
        update = { view ->
            view.contentDescription = description
            view.applyColors(accent = accent.toArgb(), track = track.toArgb())
        },
    )
}

/**
 * A transfer line's bar: the share of its batch's bytes done that [share] holds, filled in the
 * accent over a track of the accent's tint, or a moving bar while there is no share to show, as the
 * line for syncing has none. Screen readers name it [description], the line's words. The bar reads
 * [share]'s latest value each display frame while core sends new values for it, so arriving bytes
 * recompose nothing.
 */
@Composable
internal fun ShareBar(
    share: LatestSource<Double>?,
    description: String,
    modifier: Modifier = Modifier,
) {
    if (share == null) {
        MovingBar(description = description, modifier = modifier)
        return
    }
    val accent = MaterialTheme.colorScheme.primary
    val track = accent.copy(alpha = ThemeOpacity.tintStrong)
    AndroidView(
        modifier = modifier.fillMaxWidth(),
        factory = { context -> AndroidProgressBarView(context) },
        update = { view ->
            view.contentDescription = description
            view.applyColors(accent = accent.toArgb(), track = track.toArgb())
            view.follow(share)
        },
        onRelease = { view -> view.stopFollowing() },
    )
}
