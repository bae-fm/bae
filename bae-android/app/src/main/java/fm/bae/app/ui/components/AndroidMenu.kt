package fm.bae.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ProvideTextStyle
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * A dropdown menu as the design draws it: as wide as its widest item (its rows are [AndroidMenuEntry]s, which
 * have no least width), 4dp corners on the elevated colour, its items' words medium 14 over a 20 line, where
 * Material's are regular 16.
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
    AndroidMenuEntry(
        text = { Text(label) },
        onClick = {
            close()
            onClick()
        },
    )
}

/**
 * One row of a menu, as wide as its words need: a menu is as wide as its longest item, so unlike Material's
 * `DropdownMenuItem`, whose rows are at least 112 dp wide, a short menu has no empty space after its items. The
 * words are in the menu's item style, [trailingIcon] ends the row, and a disabled row is dimmed.
 */
@Composable
internal fun AndroidMenuEntry(
    text: @Composable () -> Unit,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    trailingIcon: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier =
            modifier
                .clickable(enabled = enabled, onClick = onClick)
                .fillMaxWidth()
                .heightIn(min = MENU_ITEM_MIN_HEIGHT)
                .padding(horizontal = MENU_ITEM_EDGE)
                .alpha(if (enabled) 1f else DISABLED_ALPHA),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CompositionLocalProvider(LocalContentColor provides MaterialTheme.colorScheme.onSurface) {
            ProvideTextStyle(ThemeText.androidLabel.style) {
                Box(modifier = Modifier.weight(1f, fill = false)) { text() }
            }
        }
        trailingIcon?.let { icon ->
            CompositionLocalProvider(LocalContentColor provides MaterialTheme.colorScheme.onSurfaceVariant) {
                Box(modifier = Modifier.padding(start = MENU_ITEM_EDGE)) { icon() }
            }
        }
    }
}

private val MENU_ITEM_MIN_HEIGHT = 48.dp
private val MENU_ITEM_EDGE = 12.dp

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
