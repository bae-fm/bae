package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * One row of a [RowCard]: the [title] with an optional [subtitle] under it,
 * whose [tone] colours it and puts a dot before it, and [trailing] content at
 * the far end. [action] makes the row itself the action ("Add device", last in
 * its card), with [leadingIcon] before it when it adds to the list; [selected]
 * tints the row for the one choice made. A row with an [onClick] opens or
 * changes something, which [onClickLabel] names for screen readers.
 */
@Composable
internal fun CardRow(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    tone: StatusTone? = null,
    leadingIcon: ImageVector? = null,
    action: Boolean = false,
    destructive: Boolean = false,
    selected: Boolean = false,
    onClick: (() -> Unit)? = null,
    onClickLabel: String? = null,
    trailing: (@Composable RowScope.() -> Unit)? = null,
) {
    val accent = MaterialTheme.colorScheme.primary
    Row(
        modifier =
            modifier
                .cardRowDivider()
                .fillMaxWidth()
                .background(if (selected) accent.copy(alpha = ThemeOpacity.tint) else Color.Transparent)
                .then(
                    if (onClick !=
                        null
                    ) {
                        Modifier.clickable(onClickLabel = onClickLabel, role = Role.Button, onClick = onClick)
                    } else {
                        Modifier
                    },
                ).defaultMinSize(minHeight = if (subtitle == null) ONE_LINE_HEIGHT else TWO_LINE_HEIGHT)
                .padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.group),
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.edge),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        leadingIcon?.let {
            Icon(
                imageVector = it,
                contentDescription = null,
                tint = if (action) accent else MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.size(ThemeIcon.large),
            )
        }
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = title,
                style = ThemeText.rowTitle.style,
                color =
                    when {
                        destructive -> MaterialTheme.colorScheme.error
                        action -> accent
                        else -> MaterialTheme.colorScheme.onSurface
                    },
            )
            subtitle?.let { Subtitle(it, tone) }
        }
        trailing?.invoke(this)
    }
}

@Composable
private fun Subtitle(
    text: String,
    tone: StatusTone?,
) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.compact),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val color = tone?.color ?: MaterialTheme.colorScheme.onSurfaceVariant
        if (tone != null) Box(modifier = Modifier.size(DOT).background(color, CircleShape))
        Text(
            text = text,
            style = ThemeText.body.style,
            color = color,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

private val ONE_LINE_HEIGHT = 56.dp
private val TWO_LINE_HEIGHT = 72.dp
private val DOT = 8.dp
