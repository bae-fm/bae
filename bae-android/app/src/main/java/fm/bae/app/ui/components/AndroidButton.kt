package fm.bae.app.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.height
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.LocalPrimaryFill

/** How loud a button is: [FILLED] is the action a screen is for, [OUTLINED] sits beside it, [TONAL] stands alone. */
enum class ButtonKind { FILLED, TONAL, OUTLINED, TEXT }

/** [DESTRUCTIVE] is in the danger colour; [NEUTRAL] is in the screen's text colour, beside a destructive one. */
enum class ButtonTone { DEFAULT, DESTRUCTIVE, NEUTRAL }

/**
 * Material's button of the [kind] given, with the shared primary fill for a filled one and no added
 * elevation. A [ButtonTone.DESTRUCTIVE] button is the danger colour, and the [ButtonTone.NEUTRAL] one
 * beside it (Cancel, Keep) the screen's text colour, so the accent and the danger colour never sit side
 * by side. [large] is the welcome screen's size.
 */
@Composable
fun AndroidButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    kind: ButtonKind = ButtonKind.FILLED,
    tone: ButtonTone = ButtonTone.DEFAULT,
    large: Boolean = false,
    enabled: Boolean = true,
    content: @Composable RowScope.() -> Unit,
) {
    val sized = if (large) modifier.height(LARGE_HEIGHT) else modifier
    val toneColor = tone.color()
    when (kind) {
        ButtonKind.FILLED -> {
            Button(
                onClick = onClick,
                modifier = sized,
                enabled = enabled,
                colors = filledColors(tone),
                elevation = null,
                content = content,
            )
        }

        ButtonKind.TONAL -> {
            FilledTonalButton(onClick = onClick, modifier = sized, enabled = enabled, content = content)
        }

        ButtonKind.OUTLINED -> {
            if (toneColor == null) {
                OutlinedButton(onClick = onClick, modifier = sized, enabled = enabled, content = content)
            } else {
                OutlinedButton(
                    onClick = onClick,
                    modifier = sized,
                    enabled = enabled,
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = toneColor),
                    border = BorderStroke(1.dp, toneColor),
                    content = content,
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
                content = content,
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

private val LARGE_HEIGHT = 48.dp
