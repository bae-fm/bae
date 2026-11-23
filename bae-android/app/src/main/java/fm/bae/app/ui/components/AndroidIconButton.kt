package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeOpacity

/** The colour of an [AndroidIconButton]'s glyph. */
internal enum class IconTone {
    /** The content colour around the button, dimmed while it is disabled. */
    DEFAULT,

    /** The secondary text colour. */
    MUTED,

    /** The accent, for a control that is on. */
    ACCENT,

    /** The warning icon colour, for a control that shows a problem waits. */
    WARNING,

    /** The colour that reads over a picture or a dark stage. */
    ON_FILL,
}

/**
 * An icon button: [icon] in [tone], at [glyph] when given, else the icon's own size. [box] gives the
 * button that side instead of the usual 48dp, as a bar or the full player draws it; Compose still
 * takes a press up to 48dp from its centre. [tonal] makes the button a 32dp disc tinted with the accent, with the
 * glyph in [tone], for a control inside a line.
 */
@Composable
internal fun AndroidIconButton(
    icon: ImageVector,
    contentDescription: String?,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    tone: IconTone = IconTone.DEFAULT,
    glyph: Dp? = null,
    box: Dp? = null,
    tonal: Boolean = false,
) {
    // A tonal button is the 32dp disc, as the board draws it; Compose takes a press out to 48dp from its centre.
    AndroidIconButton(
        onClick = onClick,
        modifier =
            if (tonal) {
                modifier.background(MaterialTheme.colorScheme.primary.copy(alpha = ThemeOpacity.tint), CircleShape)
            } else {
                modifier
            },
        enabled = enabled,
        box = if (tonal) TONAL_DISC else box,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = contentDescription,
            tint = tone.color(),
            modifier = if (glyph != null) Modifier.size(glyph) else Modifier,
        )
    }
}

/** An icon button that draws its own [content], such as a glyph that moves; [box] as above. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun AndroidIconButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    box: Dp? = null,
    content: @Composable () -> Unit,
) {
    CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides Dp.Unspecified) {
        IconButton(
            onClick = onClick,
            modifier = modifier.size(box ?: ICON_BUTTON_BOX),
            enabled = enabled,
            content = content,
        )
    }
}

/** The glyph's colour, read inside the button so that [IconTone.DEFAULT] takes the button's disabled colour. */
@Composable
private fun IconTone.color(): Color =
    when (this) {
        IconTone.DEFAULT -> LocalContentColor.current
        IconTone.MUTED -> MaterialTheme.colorScheme.onSurfaceVariant
        IconTone.ACCENT -> MaterialTheme.colorScheme.primary
        IconTone.WARNING -> StatusTone.WARNING.iconColor
        IconTone.ON_FILL -> BaeTheme.colors.onFill
    }

/** The side of an icon button that gives no [box], as the design draws it. */
private val ICON_BUTTON_BOX = 48.dp

/** The side of the bar's buttons, as the design draws them. */
internal val ICON_BUTTON_BAR_BOX = 40.dp

private val TONAL_DISC = 32.dp
