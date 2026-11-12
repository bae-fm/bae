package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.components.AndroidTopBar

/**
 * An album's, artist's or composer's page: the back arrow in its bar, with the page's own [actions] at the
 * end, over [content], then the [nowPlayingBar].
 */
@Composable
internal fun AndroidDetailScreen(
    onBack: () -> Unit,
    nowPlayingBar: @Composable () -> Unit,
    modifier: Modifier = Modifier.fillMaxSize(),
    actions: @Composable RowScope.() -> Unit = {},
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(modifier = modifier) {
        AndroidTopBar(onBack = onBack, actions = actions)
        content()
        nowPlayingBar()
    }
}
