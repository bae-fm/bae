package fm.bae.app.ui.sync

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import fm.bae.app.ui.components.AndroidProgressBarView
import fm.bae.app.ui.components.MovingBar
import fm.bae.fastvalues.LatestSource

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
    AndroidView(
        modifier = modifier.fillMaxWidth(),
        factory = { context -> AndroidProgressBarView(context) },
        update = { view ->
            view.contentDescription = description
            view.applyAccent(accent)
            view.follow(share)
        },
        onRelease = { view -> view.stopFollowing() },
    )
}
