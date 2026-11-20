package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

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
        shape = RoundedCornerShape(MENU_RADIUS),
        containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
    ) {
        MenuWords(content)
    }
}

/**
 * A menu drawn open in the place it is composed, as [AndroidMenu] draws it in its popup, for a shot: a popup is a
 * window of its own, which the semantics tree of a shot does not hold.
 */
@Composable
fun AndroidMenuInPlace(
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(
        shape = RoundedCornerShape(MENU_RADIUS),
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        tonalElevation = MenuDefaults.TonalElevation,
        shadowElevation = MenuDefaults.ShadowElevation,
        modifier = modifier,
    ) {
        Column(
            modifier = Modifier.padding(vertical = MENU_PADDING).width(IntrinsicSize.Max),
        ) {
            MenuWords(content)
        }
    }
}

/** A menu item sets its words in Material's body large, so the menu's own is that style. */
@Composable
private fun MenuWords(content: @Composable ColumnScope.() -> Unit) {
    val typography = MaterialTheme.typography
    val itemText =
        typography.bodyLarge.copy(
            fontSize = 14.sp,
            lineHeight = 20.sp,
            fontWeight = FontWeight.Medium,
            letterSpacing = 0.sp,
        )
    MaterialTheme(typography = typography.copy(bodyLarge = itemText)) { Column { content() } }
}

private val MENU_RADIUS = 4.dp
private val MENU_PADDING = 8.dp
