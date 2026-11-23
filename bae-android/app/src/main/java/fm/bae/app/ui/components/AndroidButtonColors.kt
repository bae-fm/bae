package fm.bae.app.ui.components

import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

@Composable
internal fun tonalColors(): ButtonColors =
    ButtonDefaults.filledTonalButtonColors(
        disabledContainerColor = disabledContainer(),
        disabledContentColor = disabledContent(),
    )

@Composable
internal fun outlinedColors(tone: Color): ButtonColors =
    ButtonDefaults.outlinedButtonColors(contentColor = tone, disabledContentColor = disabledContent())

/** A text button's colours: the accent unless a [tone] says another, whatever colour surrounds the button. */
@Composable
internal fun textColors(tone: Color?): ButtonColors =
    ButtonDefaults.textButtonColors(
        contentColor = tone ?: MaterialTheme.colorScheme.primary,
        disabledContentColor = disabledContent(),
    )

/** A disabled button's words: the screen's text at 38%, whatever the kind and tone. */
@Composable
internal fun disabledContent(): Color = MaterialTheme.colorScheme.onSurface.copy(alpha = DISABLED_ALPHA)

/** A disabled filled or tonal button's fill, and a plain outlined one's outline: the screen's text at 12%. */
@Composable
internal fun disabledContainer(): Color = MaterialTheme.colorScheme.onSurface.copy(alpha = DISABLED_FILL_ALPHA)

private const val DISABLED_FILL_ALPHA = 0.12f
