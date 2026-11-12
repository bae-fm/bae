package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/** A screen: its bar, with the [title] and the way back, over [content] in a column of [modifier]'s shape. */
@Composable
internal fun AndroidScreen(
    title: String,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(modifier = modifier) {
        AndroidTopBar(onBack = onBack, title = title)
        content()
    }
}

/**
 * A screen's bar, as the design draws it: 64 dp on the page's own colour, the way back, then the [title] if the
 * screen has one, and at the end the screen's own [actions].
 */
@Composable
internal fun AndroidTopBar(
    onBack: () -> Unit,
    title: String? = null,
    actions: @Composable RowScope.() -> Unit = {},
) {
    Row(
        modifier = Modifier.fillMaxWidth().height(64.dp).padding(horizontal = ThemeSpace.inline),
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = onBack) {
            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
        }
        if (title != null) {
            Text(text = title, style = ThemeText.androidBarTitle.style)
        }
        Spacer(modifier = Modifier.weight(1f))
        actions()
    }
}
