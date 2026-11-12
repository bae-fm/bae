package fm.bae.app.ui.components

import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import fm.bae.app.ui.appearance.ThemeIcon

/**
 * An icon that sits level with a line of text, such as a notice's × or a download's cancel: the
 * glyph is [ThemeIcon.medium] wide, and its touch area overhangs the row's padding.
 */
@Composable
internal fun AndroidInlineIconButton(
    icon: ImageVector,
    contentDescription: String,
    onClick: () -> Unit,
    tint: Color = MaterialTheme.colorScheme.onSurfaceVariant,
) {
    IconButton(onClick = onClick, modifier = Modifier.size(ThemeIcon.medium).wrapContentSize(unbounded = true)) {
        Icon(
            imageVector = icon,
            contentDescription = contentDescription,
            tint = tint,
            modifier = Modifier.size(ThemeIcon.medium),
        )
    }
}
