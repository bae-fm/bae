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
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * One row of a list, in a [AndroidCard] or a sheet: the [title] with an optional [subtitle] under it,
 * whose [tone] colours it and puts a dot before it, and [trailing] content at the far end. [action]
 * makes the row itself the action ("Add device", last in its card), with [leadingIcon] before it when
 * it adds to the list; [selected] tints the row for the one choice made; a [value] is the title's value
 * at the row's far end, in the secondary colour. The [subtitle] wraps, read whole, as Material's supporting text does;
 * [subtitleContent] draws the line itself where its words change as the row runs. A [nested] row is a
 * setting that belongs to the switch row above it: joined to it in one tile and set in. A row that
 * is not [enabled] shows dimmed and does nothing. A row with an [onClick] opens or changes something,
 * which [onClickLabel] names for screen readers.
 */
@Composable
internal fun AndroidRow(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    value: String? = null,
    subtitleContent: (@Composable () -> Unit)? = null,
    tone: StatusTone? = null,
    leadingIcon: ImageVector? = null,
    action: Boolean = false,
    destructive: Boolean = false,
    selected: Boolean = false,
    nested: Boolean = false,
    enabled: Boolean = true,
    onClick: (() -> Unit)? = null,
    onClickLabel: String? = null,
    trailing: (@Composable RowScope.() -> Unit)? = null,
) {
    val accent = MaterialTheme.colorScheme.primary
    Row(
        modifier =
            modifier.cardRowSurface(
                nested = nested,
                twoLines = subtitle != null || subtitleContent != null,
                selected = selected,
                enabled = enabled,
                onClick = onClick,
                onClickLabel = onClickLabel,
            ),
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
            subtitleContent?.invoke()
        }
        value?.let {
            Text(text = it, style = ThemeText.body.style, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        trailing?.invoke(this)
    }
}

/** The row's divider, fill, dimming, tap, size and padding; [nested] sets it in under the row above. */
@Composable
private fun Modifier.cardRowSurface(
    nested: Boolean,
    twoLines: Boolean,
    selected: Boolean,
    enabled: Boolean,
    onClick: (() -> Unit)?,
    onClickLabel: String?,
): Modifier {
    val accent = MaterialTheme.colorScheme.primary
    val minHeight =
        when {
            nested -> 0.dp
            twoLines -> TWO_LINE_HEIGHT
            else -> ONE_LINE_HEIGHT
        }
    return this
        .then(if (nested) Modifier.joinsTileAbove() else Modifier)
        .fillMaxWidth()
        .alpha(if (enabled) 1f else DISABLED_ALPHA)
        .background(if (selected) accent.copy(alpha = ThemeOpacity.tint) else Color.Transparent)
        .then(
            if (onClick != null) {
                Modifier.clickable(
                    enabled = enabled,
                    onClickLabel = onClickLabel,
                    role = Role.Button,
                    onClick = onClick,
                )
            } else {
                Modifier
            },
        ).defaultMinSize(minHeight = minHeight)
        .padding(
            start = if (nested) ThemeSpace.edge + ThemeSpace.section else ThemeSpace.edge,
            top = if (nested) 0.dp else ThemeSpace.group,
            end = ThemeSpace.edge,
            bottom = ThemeSpace.group,
        )
}

@Composable
private fun Subtitle(
    text: String,
    tone: StatusTone?,
) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.compact),
        verticalAlignment = Alignment.Top,
    ) {
        val color = tone?.color ?: MaterialTheme.colorScheme.onSurfaceVariant
        if (tone != null) FirstLineBox { Box(modifier = Modifier.size(DOT).background(color, CircleShape)) }
        Text(
            text = text,
            style = ThemeText.body.style,
            color = color,
        )
    }
}

private val ONE_LINE_HEIGHT = 56.dp
private val TWO_LINE_HEIGHT = 72.dp
private val DOT = 8.dp

/** Material's alpha for a disabled control's content. */
private const val DISABLED_ALPHA = 0.38f
