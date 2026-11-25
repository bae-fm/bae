package fm.bae.app.ui.components

import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.LocalAppearanceStore
import fm.bae.app.ui.appearance.appearanceColorScheme

@Composable
internal fun tonalColors(): ButtonColors =
    ButtonDefaults.filledTonalButtonColors(
        disabledContainerColor = disabledContainer(),
        disabledContentColor = disabledContent(),
    )

/** A tonal button on the gallery's backdrop, which is dark in both modes: dark mode's tile under white words. */
@Composable
internal fun stageTonalColors(): ButtonColors {
    val preferences by LocalAppearanceStore.current.preferences.collectAsState()
    val dark = remember(preferences) { appearanceColorScheme(preferences, dark = true) }
    return ButtonDefaults.filledTonalButtonColors(
        containerColor = dark.secondaryContainer,
        contentColor = BaeTheme.colors.onFill,
        disabledContainerColor = disabledContainer(),
        disabledContentColor = disabledContent(),
    )
}

/** A text button on the gallery's backdrop, which is dark in both modes: dark mode's accent. */
@Composable
internal fun stageTextColors(): ButtonColors {
    val preferences by LocalAppearanceStore.current.preferences.collectAsState()
    val dark = remember(preferences) { appearanceColorScheme(preferences, dark = true) }
    return ButtonDefaults.textButtonColors(contentColor = dark.primary, disabledContentColor = disabledContent())
}

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
