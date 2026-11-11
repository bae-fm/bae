package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.ButtonKind

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
        LibraryDetailTopBar(onBack = onBack, actions = actions)
        content()
        nowPlayingBar()
    }
}

/**
 * A screen inside the library: its bar holds the one way back, labeled with
 * where it goes, and at its end the screen's own [actions], if it has any.
 */
@Composable
private fun LibraryDetailTopBar(
    onBack: () -> Unit,
    actions: @Composable RowScope.() -> Unit = {},
) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(ThemeSpace.related),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // One button: the arrow and the label read together as "Back, Library".
            AndroidButton(kind = ButtonKind.TEXT, onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
                Spacer(modifier = Modifier.width(ThemeSpace.inline))
                Text(text = stringResource(R.string.library))
            }
            Spacer(modifier = Modifier.weight(1f))
            actions()
        }
    }
}
