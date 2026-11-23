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
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.invisibleToUser
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import androidx.compose.ui.semantics.onClick as semanticsOnClick

/**
 * What a tap on a row does, drawn at its far end: [CHEVRON] opens a page, [EDIT] a dialog that changes the value,
 * [EXPAND_MORE] shows more rows and [EXPAND_LESS] hides them again.
 */
internal enum class RowIcon(
    val image: ImageVector,
    val size: Dp,
) {
    CHEVRON(SymbolIcons.ChevronRight, ThemeIcon.large),
    EDIT(SymbolIcons.Edit, ThemeIcon.medium),
    EXPAND_MORE(SymbolIcons.ExpandMore, ThemeIcon.large),
    EXPAND_LESS(SymbolIcons.ExpandLess, ThemeIcon.large),
}

/** Where a row sits in its card. A row given one draws the tile the card would draw behind it. */
internal enum class RowPlace(
    val top: Boolean,
    val bottom: Boolean,
) {
    FIRST(top = true, bottom = false),
    MIDDLE(top = false, bottom = false),
    LAST(top = false, bottom = true),
    ALONE(top = true, bottom = true),
}

/** A row's switch: whether it is [checked], and what a tap anywhere on the row does, [onChange]. */
internal class RowSwitch(
    val checked: Boolean,
    val onChange: (Boolean) -> Unit,
)

/**
 * One row of a list, in a [AndroidCard] or a sheet: the [title] with an optional [subtitle] under it,
 * whose [tone] colours it and puts a dot before it, and [trailing] content at the far end. [action]
 * makes the row itself the action ("Add device", last in its card), with [leadingIcon] before it when
 * it adds to the list; an [avatar] leads the row of a person, with their picture or initials;
 * [icon] says what a tap does, and [tag] is a short chip after the title, a role or a state;
 * [selected] tints the row for the one choice made; a [value] is the title's value
 * at the row's far end, in the secondary colour. The [subtitle] wraps, read whole, as Material's supporting text does;
 * [subtitleContent] draws the line itself where its words change as the row runs. A [nested] row is a
 * setting that belongs to the switch row above it: joined to it in one tile and set in. A row that is
 * [followed] by lines of its own tile (a status with its cause under it) has no minimum height and only 4dp
 * below. A row with a [place] draws the card's tile for that place behind itself.
 * A row that is not [enabled] shows dimmed and does nothing. A row with an [onClick] opens or changes something,
 * which [onClickLabel] names for screen readers. A row with a [switch] is a setting turned on or off:
 * the whole row toggles, its title names the switch, and the switch is drawn at the far end.
 */
@Composable
internal fun AndroidRow(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    value: String? = null,
    tag: String? = null,
    icon: RowIcon? = null,
    subtitleContent: (@Composable () -> Unit)? = null,
    tone: StatusTone? = null,
    leadingIcon: ImageVector? = null,
    avatar: AvatarFace? = null,
    action: Boolean = false,
    destructive: Boolean = false,
    selected: Boolean = false,
    nested: Boolean = false,
    followed: Boolean = false,
    place: RowPlace? = null,
    enabled: Boolean = true,
    onClick: (() -> Unit)? = null,
    onClickLabel: String? = null,
    switch: RowSwitch? = null,
    trailing: (@Composable RowScope.() -> Unit)? = null,
) {
    val row: @Composable (SwitchControl?) -> Unit = { control ->
        // A row that ends in controls of its own (a ⋮ menu) gives its button to its words alone, so a button
        // never holds another one; a layer behind the row still takes a tap anywhere on it.
        val wordsTap = onClick.takeIf { trailing != null && control == null }
        val content: @Composable RowScope.() -> Unit = {
            RowContent(
                title = title,
                subtitle = subtitle,
                value = value,
                tag = tag,
                icon = icon,
                subtitleContent = subtitleContent,
                tone = tone,
                leadingIcon = leadingIcon,
                avatar = avatar,
                action = action,
                destructive = destructive,
                words = wordsTap?.let { Modifier.wordsButton(enabled, onClickLabel, it) } ?: Modifier,
                control = control,
                trailing = trailing,
            )
        }
        val surface =
            modifier.cardRowSurface(
                nested = nested,
                place = place,
                selected = selected,
                enabled = enabled,
                onClick = onClick.takeIf { wordsTap == null },
                onClickLabel = onClickLabel,
                switchAction = control?.action ?: Modifier,
            )
        val twoLines = subtitle != null || subtitleContent != null
        val inset = Modifier.rowInset(nested = nested, followed = followed, twoLines = twoLines)
        if (wordsTap != null) {
            TapLayerRow(surface, inset, enabled, wordsTap, content)
        } else {
            Row(
                modifier = surface.then(inset),
                horizontalArrangement = Arrangement.spacedBy(ThemeSpace.edge),
                verticalAlignment = Alignment.CenterVertically,
                content = content,
            )
        }
    }
    if (switch == null) {
        row(null)
    } else {
        AndroidSwitch(
            checked = switch.checked,
            onCheckedChange = switch.onChange,
            content = row,
        )
    }
}

/** A row whose button is its words alone: a layer behind [content] takes a tap anywhere on the row. */
@Composable
private fun TapLayerRow(
    surface: Modifier,
    inset: Modifier,
    enabled: Boolean,
    onClick: () -> Unit,
    content: @Composable RowScope.() -> Unit,
) {
    Box(modifier = surface) {
        Box(
            modifier =
                Modifier
                    .matchParentSize()
                    .clickable(enabled = enabled, onClick = onClick)
                    .semantics { invisibleToUser() },
        )
        Row(
            modifier = inset.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.edge),
            verticalAlignment = Alignment.CenterVertically,
            content = content,
        )
    }
}

/** The button of a row's words: one node, as Compose merges a clickable row, but without the row's controls. */
private fun Modifier.wordsButton(
    enabled: Boolean,
    onClickLabel: String?,
    onClick: () -> Unit,
): Modifier =
    semantics(mergeDescendants = true) {
        role = Role.Button
        if (!enabled) disabled()
        semanticsOnClick(label = onClickLabel) {
            onClick()
            true
        }
    }

/** What a row draws: its lead, its words ([words] makes them the button), its value, controls and icon. */
@Composable
private fun RowScope.RowContent(
    title: String,
    subtitle: String?,
    value: String?,
    tag: String?,
    icon: RowIcon?,
    subtitleContent: (@Composable () -> Unit)?,
    tone: StatusTone?,
    leadingIcon: ImageVector?,
    avatar: AvatarFace?,
    action: Boolean,
    destructive: Boolean,
    words: Modifier,
    control: SwitchControl?,
    trailing: (@Composable RowScope.() -> Unit)?,
) {
    leadingIcon?.let {
        Icon(
            imageVector = it,
            contentDescription = null,
            tint = if (action) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(ThemeIcon.large),
        )
    }
    avatar?.let { Avatar(face = it, size = AVATAR_SIZE) }
    Column(modifier = Modifier.weight(1f).then(words)) {
        Title(title, tag, action = action, destructive = destructive)
        subtitle?.let { Subtitle(it, tone) }
        subtitleContent?.invoke()
    }
    value?.let {
        Text(text = it, style = ThemeText.body.style, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
    control?.indicator?.invoke()
    trailing?.invoke(this)
    icon?.let {
        Icon(
            imageVector = it.image,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(it.size),
        )
    }
}

/** The title, regular (an action's is medium), with its [tag] as a chip after it. */
@Composable
private fun Title(
    title: String,
    tag: String?,
    action: Boolean,
    destructive: Boolean,
) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = title,
            style = if (action) ThemeText.rowTitle.style else ThemeText.androidRowTitle.style,
            color =
                when {
                    destructive -> MaterialTheme.colorScheme.error
                    action -> MaterialTheme.colorScheme.primary
                    else -> MaterialTheme.colorScheme.onSurface
                },
        )
        tag?.let { StatusChip(it) }
    }
}

/** The row's fill, dimming and tap; [nested] joins it to the tile above. */
@Composable
private fun Modifier.cardRowSurface(
    nested: Boolean,
    place: RowPlace?,
    selected: Boolean,
    enabled: Boolean,
    onClick: (() -> Unit)?,
    onClickLabel: String?,
    switchAction: Modifier,
): Modifier {
    val accent = MaterialTheme.colorScheme.primary
    return this
        .then(if (nested) Modifier.joinsTileAbove() else Modifier)
        .fillMaxWidth()
        .then(
            if (place != null) {
                Modifier.background(MaterialTheme.colorScheme.surfaceContainer, tileShape(place.top, place.bottom))
            } else {
                Modifier
            },
        ).alpha(if (enabled) 1f else DISABLED_ALPHA)
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
        ).then(switchAction)
}

/** The row's size and padding: [nested] sets it in under the row above, [followed] gives it no minimum height. */
private fun Modifier.rowInset(
    nested: Boolean,
    followed: Boolean,
    twoLines: Boolean,
): Modifier {
    val minHeight =
        when {
            nested || followed -> 0.dp
            twoLines -> TWO_LINE_HEIGHT
            else -> ONE_LINE_HEIGHT
        }
    return defaultMinSize(minHeight = minHeight).padding(
        start = if (nested) ThemeSpace.edge + ThemeSpace.section else ThemeSpace.edge,
        top = if (nested) 0.dp else ThemeSpace.group,
        end = ThemeSpace.edge,
        bottom = if (followed) ThemeSpace.inline else ThemeSpace.group,
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
        if (tone != null) FirstLine { Box(modifier = Modifier.size(DOT).background(color, CircleShape)) }
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
private val AVATAR_SIZE = 40.dp

/** Material's alpha for a disabled control's content. */
internal const val DISABLED_ALPHA = 0.38f
