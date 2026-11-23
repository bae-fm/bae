package fm.bae.app.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.LocalPrimaryFill
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace

/** How loud a button is: [FILLED] is the action a screen is for, [OUTLINED] sits beside it, [TONAL] stands alone. */
enum class ButtonKind { FILLED, TONAL, OUTLINED, TEXT }

/** [DESTRUCTIVE] is in the danger colour; [NEUTRAL] is in the screen's text colour, beside a destructive one. */
enum class ButtonTone { DEFAULT, DESTRUCTIVE, NEUTRAL }

/**
 * Where a [ButtonKind.TEXT] button on its own line lines its words up: [TEXT] flush with the plain text
 * above it, [FIELD] with the text inside the field above it, 16dp in.
 */
enum class ButtonAlign { TEXT, FIELD }

/**
 * Material's button of the [kind] given, with the shared primary fill for a filled one and no added
 * elevation. A [ButtonTone.DESTRUCTIVE] button is the danger colour, and the [ButtonTone.NEUTRAL] one
 * beside it (Cancel, Keep) the screen's text colour, so the accent and the danger colour never sit side
 * by side. [icon] leads the words, as the album page's pin control draws it. [large] is the welcome
 * screen's size. [alignTo] moves a text button's words over the text or field above it, cancelling the
 * button's own 12dp side padding.
 */
@Composable
fun AndroidButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    kind: ButtonKind = ButtonKind.FILLED,
    tone: ButtonTone = ButtonTone.DEFAULT,
    large: Boolean = false,
    alignTo: ButtonAlign? = null,
    enabled: Boolean = true,
    icon: ImageVector? = null,
    content: @Composable RowScope.() -> Unit,
) {
    val aligned =
        when (alignTo) {
            null -> modifier
            ButtonAlign.TEXT -> modifier.offset(x = -TEXT_BUTTON_PADDING)
            ButtonAlign.FIELD -> modifier.offset(x = FIELD_TEXT_INSET - TEXT_BUTTON_PADDING)
        }
    val sized = (if (large) aligned.height(LARGE_HEIGHT) else aligned).withoutMinimumWidth()
    val labelled = withIcon(icon, content)
    val toneColor = tone.color()
    when (kind) {
        ButtonKind.FILLED -> {
            Button(
                onClick = onClick,
                modifier = sized,
                enabled = enabled,
                colors = filledColors(tone),
                elevation = null,
                content = labelled,
            )
        }

        ButtonKind.TONAL -> {
            FilledTonalButton(
                onClick = onClick,
                modifier = sized,
                enabled = enabled,
                colors = tonalColors(),
                content = labelled,
            )
        }

        ButtonKind.OUTLINED -> {
            if (toneColor == null) {
                PlainOutlinedButton(onClick, sized, enabled, labelled)
            } else {
                OutlinedButton(
                    onClick = onClick,
                    modifier = sized,
                    enabled = enabled,
                    colors = outlinedColors(toneColor),
                    border = BorderStroke(1.dp, toneColor),
                    content = labelled,
                )
            }
        }

        ButtonKind.TEXT -> {
            TextButton(
                onClick = onClick,
                modifier = sized,
                enabled = enabled,
                colors = textColors(toneColor),
                content = labelled,
            )
        }
    }
}

@Composable
private fun filledColors(tone: ButtonTone): ButtonColors =
    if (tone == ButtonTone.DESTRUCTIVE) {
        ButtonDefaults.buttonColors(
            containerColor = MaterialTheme.colorScheme.error,
            contentColor = MaterialTheme.colorScheme.background,
            disabledContainerColor = disabledContainer(),
            disabledContentColor = disabledContent(),
        )
    } else {
        ButtonDefaults.buttonColors(
            containerColor = LocalPrimaryFill.current,
            contentColor = BaeTheme.colors.onFill,
            disabledContainerColor = disabledContainer(),
            disabledContentColor = disabledContent(),
        )
    }

/** The tone's text colour, or null where the button's own is kept. */
@Composable
private fun ButtonTone.color(): Color? =
    when (this) {
        ButtonTone.DEFAULT -> null
        ButtonTone.DESTRUCTIVE -> MaterialTheme.colorScheme.error
        ButtonTone.NEUTRAL -> MaterialTheme.colorScheme.onSurface
    }

/** The button's label [content], after the [icon] and a gap when there is one. */
private fun withIcon(
    icon: ImageVector?,
    content: @Composable RowScope.() -> Unit,
): @Composable RowScope.() -> Unit =
    if (icon == null) {
        content
    } else {
        {
            Icon(icon, contentDescription = null, modifier = Modifier.size(ThemeIcon.medium))
            Spacer(modifier = Modifier.width(ThemeSpace.related))
            content()
        }
    }

/** An outlined button in no tone: its words in the text colour, inside a 1dp edge in the secondary one. */
@Composable
private fun PlainOutlinedButton(
    onClick: () -> Unit,
    modifier: Modifier,
    enabled: Boolean,
    content: @Composable RowScope.() -> Unit,
) {
    OutlinedButton(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        colors = outlinedColors(MaterialTheme.colorScheme.onSurface),
        border = BorderStroke(1.dp, outlineColor(enabled)),
        content = content,
    )
}

/** The outline of a plain outlined button: the secondary text colour, and Material's 12% of the text's when off. */
@Composable
private fun outlineColor(enabled: Boolean): Color =
    if (enabled) {
        MaterialTheme.colorScheme.onSurfaceVariant
    } else {
        disabledContainer()
    }

/**
 * Material gives each button a 58dp minimum width, which makes a short label such as Save or Pair wider than
 * its padding and words (the board draws no minimum). Material applies its minimum only while the incoming
 * minimum width is 0, so this asks the button for at least 1px, which leaves every real width unchanged.
 */
private fun Modifier.withoutMinimumWidth(): Modifier =
    layout { measurable, constraints ->
        val placeable = measurable.measure(constraints.copy(minWidth = maxOf(constraints.minWidth, 1)))
        layout(placeable.width, placeable.height) { placeable.place(0, 0) }
    }

private val LARGE_HEIGHT = 48.dp

/** Material's text button pads its words 12dp from each side. */
internal val TEXT_BUTTON_PADDING = 12.dp

/** A text field's text sits 16dp in from its edge. */
private val FIELD_TEXT_INSET = 16.dp
