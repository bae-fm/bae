package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
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

    /** The colour that reads over a picture or a dark stage. */
    ON_FILL,
}

/**
 * An icon button: [icon] in [tone], at [glyph] when given, else the icon's own size. [box] gives the
 * button that side instead of Material's 48dp layout, as a bar or the full player draws it; Compose still
 * takes a press up to 48dp from its centre. [tonal] draws the glyph on a 32dp disc tinted with [tone],
 * for a control inside a line, with the usual 48dp touch area around it.
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
    if (tonal) {
        IconButton(
            onClick = onClick,
            modifier = modifier.size(TONAL_DISC).wrapContentSize(unbounded = true),
            enabled = enabled,
        ) {
            val tint = tone.color()
            Box(
                modifier = Modifier.size(TONAL_DISC).background(tint.copy(alpha = ThemeOpacity.tint), CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    imageVector = icon,
                    contentDescription = contentDescription,
                    tint = tint,
                    modifier = Modifier.size(glyph ?: TONAL_GLYPH),
                )
            }
        }
        return
    }
    AndroidIconButton(onClick = onClick, modifier = modifier, enabled = enabled, box = box) {
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
    if (box == null) {
        IconButton(onClick = onClick, modifier = modifier, enabled = enabled, content = content)
    } else {
        CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides Dp.Unspecified) {
            IconButton(onClick = onClick, modifier = modifier.size(box), enabled = enabled, content = content)
        }
    }
}

/** The glyph's colour, read inside the button so that [IconTone.DEFAULT] takes the button's disabled colour. */
@Composable
private fun IconTone.color(): Color =
    when (this) {
        IconTone.DEFAULT -> LocalContentColor.current
        IconTone.MUTED -> MaterialTheme.colorScheme.onSurfaceVariant
        IconTone.ACCENT -> MaterialTheme.colorScheme.primary
        IconTone.ON_FILL -> BaeTheme.colors.onFill
    }

/** The side of the bar's buttons, as the design draws them. */
internal val ICON_BUTTON_BAR_BOX = 40.dp

private val TONAL_DISC = 32.dp
private val TONAL_GLYPH = 18.dp
