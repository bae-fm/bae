package fm.bae.app.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.ProvideTextStyle
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.LocalPrimaryFill

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
 * by side. [large] is the welcome screen's size. [alignTo] moves a text button's words over the text
 * or field above it, cancelling the button's own 12dp side padding.
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
    content: @Composable RowScope.() -> Unit,
) {
    val aligned =
        when (alignTo) {
            null -> modifier
            ButtonAlign.TEXT -> modifier.offset(x = -TEXT_BUTTON_PADDING)
            ButtonAlign.FIELD -> modifier.offset(x = FIELD_TEXT_INSET - TEXT_BUTTON_PADDING)
        }
    val sized = if (large) aligned.height(LARGE_HEIGHT) else aligned
    val labelled = labelled(large, content)
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
            FilledTonalButton(onClick = onClick, modifier = sized, enabled = enabled, content = labelled)
        }

        ButtonKind.OUTLINED -> {
            if (toneColor == null) {
                PlainOutlinedButton(onClick, sized, enabled, labelled)
            } else {
                OutlinedButton(
                    onClick = onClick,
                    modifier = sized,
                    enabled = enabled,
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = toneColor),
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
                colors =
                    if (toneColor == null) {
                        ButtonDefaults.textButtonColors()
                    } else {
                        ButtonDefaults.textButtonColors(contentColor = toneColor)
                    },
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
        )
    } else {
        ButtonDefaults.buttonColors(containerColor = LocalPrimaryFill.current, contentColor = BaeTheme.colors.onFill)
    }

/** The tone's text colour, or null where the button's own is kept. */
@Composable
private fun ButtonTone.color(): Color? =
    when (this) {
        ButtonTone.DEFAULT -> null
        ButtonTone.DESTRUCTIVE -> MaterialTheme.colorScheme.error
        ButtonTone.NEUTRAL -> MaterialTheme.colorScheme.onSurface
    }

/** The button's label [content], in the large button's 15sp where it is [large] (the others' are 14). */
private fun labelled(
    large: Boolean,
    content: @Composable RowScope.() -> Unit,
): @Composable RowScope.() -> Unit =
    if (large) {
        {
            ProvideTextStyle(MaterialTheme.typography.labelLarge.copy(fontSize = LARGE_LABEL_SIZE)) { content() }
        }
    } else {
        content
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
        colors = ButtonDefaults.outlinedButtonColors(contentColor = MaterialTheme.colorScheme.onSurface),
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
        MaterialTheme.colorScheme.onSurface.copy(alpha = DISABLED_OUTLINE_ALPHA)
    }

private const val DISABLED_OUTLINE_ALPHA = 0.12f

private val LARGE_HEIGHT = 48.dp
private val LARGE_LABEL_SIZE = 15.sp

/** Material's text button pads its words 12dp from each side. */
private val TEXT_BUTTON_PADDING = 12.dp

/** A text field's text sits 16dp in from its edge. */
private val FIELD_TEXT_INSET = 16.dp
