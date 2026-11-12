package fm.bae.app.ui.components

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * A dropdown menu as the design draws it: at least [minWidth] wide (200dp unless a board says otherwise), 4dp
 * corners, its items' words medium 14 over a 20 line, where Material's are regular 16.
 */
@Composable
fun AndroidMenu(
    expanded: Boolean,
    onDismissRequest: () -> Unit,
    modifier: Modifier = Modifier,
    minWidth: Dp = MENU_MIN_WIDTH,
    content: @Composable ColumnScope.() -> Unit,
) {
    val typography = MaterialTheme.typography
    val itemText =
        typography.bodyLarge.copy(
            fontSize = 14.sp,
            lineHeight = 20.sp,
            fontWeight = FontWeight.Medium,
            letterSpacing = 0.sp,
        )
    DropdownMenu(
        expanded = expanded,
        onDismissRequest = onDismissRequest,
        modifier = modifier.widthIn(min = minWidth),
        shape = RoundedCornerShape(MENU_RADIUS),
    ) {
        // A menu item sets its words in Material's body large, so the menu's own is that style.
        MaterialTheme(typography = typography.copy(bodyLarge = itemText)) { content() }
    }
}

private val MENU_MIN_WIDTH = 200.dp
private val MENU_RADIUS = 4.dp
