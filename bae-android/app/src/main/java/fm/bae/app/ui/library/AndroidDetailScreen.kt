package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidTopBar

/**
 * An album's, artist's or composer's page: the back arrow in its bar, named [backLabel], with the page's own
 * [actions] at the end, over [content], then the [nowPlayingBar]. The content sits in a column padded by
 * [ThemeSpace.edge], unless [flush]: a page whose rows carry their own padding.
 */
@Composable
internal fun AndroidDetailScreen(
    onBack: () -> Unit,
    backLabel: String,
    nowPlayingBar: @Composable () -> Unit,
    modifier: Modifier = Modifier.fillMaxSize(),
    actions: @Composable RowScope.() -> Unit = {},
    flush: Boolean = false,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(modifier = modifier) {
        AndroidTopBar(onBack = onBack, backLabel = backLabel, actions = actions)
        if (flush) {
            content()
        } else {
            Column(modifier = Modifier.weight(1f).padding(ThemeSpace.edge), content = content)
        }
        nowPlayingBar()
    }
}
