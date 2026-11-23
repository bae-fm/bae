package fm.bae.app.ui.components

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * A dropdown menu as the design draws it: as wide as its widest item, 4dp
 * corners on the elevated colour, its items' words medium 14 over a 20 line, where Material's are regular 16.
 */
@Composable
fun AndroidMenu(
    expanded: Boolean,
    onDismissRequest: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    DropdownMenu(
        expanded = expanded,
        onDismissRequest = onDismissRequest,
        modifier = modifier,
        shape = AndroidMenuShape,
        containerColor = androidMenuColor(),
    ) {
        AndroidMenuItemStyle(content)
    }
}

/**
 * The words of a menu's items as the design draws them, in the label style. A menu item sets its words in
 * Material's body large, so the menu's own is that style.
 */
@Composable
internal fun ColumnScope.AndroidMenuItemStyle(content: @Composable ColumnScope.() -> Unit) {
    val typography = MaterialTheme.typography
    MaterialTheme(typography = typography.copy(bodyLarge = ThemeText.androidLabel.style)) { content() }
}

/** One action of a menu: a tap closes the menu with [close], then runs [onClick]. */
@Composable
internal fun AndroidMenuAction(
    label: String,
    close: () -> Unit,
    onClick: () -> Unit,
) {
    DropdownMenuItem(
        text = { Text(label) },
        onClick = {
            close()
            onClick()
        },
    )
}

/** A group's name over its items, in the secondary colour; a screen reader announces it as a heading. */
@Composable
internal fun AndroidMenuHeading(text: String) {
    Text(
        text = text,
        style = ThemeText.detail.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier =
            Modifier
                .padding(horizontal = ThemeSpace.group, vertical = ThemeSpace.inline)
                .semantics { heading() },
    )
}

/** The line between a menu's groups, with the menu spacing above and below it. */
@Composable
internal fun AndroidMenuDivider() = AndroidDivider(modifier = Modifier.padding(vertical = ThemeSpace.related))

/** The menu's corners. */
internal val AndroidMenuShape = RoundedCornerShape(4.dp)

/** The menu's colour. */
@Composable
internal fun androidMenuColor() = MaterialTheme.colorScheme.surfaceContainerHigh
