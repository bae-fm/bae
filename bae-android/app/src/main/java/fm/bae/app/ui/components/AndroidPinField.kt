package fm.bae.app.ui.components

import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeOpacity
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

private val CELL_WIDTH = 48.dp
private val CELL_HEIGHT = 56.dp
private val CELL_CORNER = RoundedCornerShape(4.dp)
private val EDGE = 1.dp
private val RING = 2.dp
private val cellDigit = ThemeText.androidFigure.style.copy(fontFeatureSettings = "tnum")

/**
 * A PIN the person reads off a receiver's screen and types on the number pad: one outlined box per digit, each
 * showing its digit as it is typed. While the field has focus, the box where the next digit goes has the accent
 * outline. The field takes digits only, keeps leading zeros and stops at [digits]. With an [error], every box is
 * outlined in red and the error is the line under them. Off ([enabled] false), the field dims and takes nothing,
 * as it does while bae pairs. A focus request on [modifier] gives the field the keyboard.
 */
@Composable
internal fun AndroidPinField(
    digits: Int,
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    error: String? = null,
    enabled: Boolean = true,
) {
    var focused by remember { mutableStateOf(false) }
    val label = stringResource(R.string.cast_pin_field)
    Column(
        modifier = modifier.alpha(if (enabled) 1f else ThemeOpacity.off),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
    ) {
        BasicTextField(
            value = value,
            enabled = enabled,
            onValueChange = { typed -> onValueChange(typed.filter { it in '0'..'9' }.take(digits)) },
            modifier =
                Modifier
                    .onFocusChanged { focused = it.isFocused }
                    .semantics { contentDescription = label },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
            decorationBox = {
                Row(
                    modifier = Modifier.clearAndSetSemantics {},
                    horizontalArrangement = Arrangement.spacedBy(ThemeSpace.group),
                ) {
                    repeat(digits) { index ->
                        // The box for the next digit, or the last box once every digit is typed.
                        val next = index == minOf(value.length, digits - 1)
                        PinCell(
                            value.getOrNull(index),
                            ringed = focused && error == null && next,
                            wrong = error != null,
                        )
                    }
                }
            },
        )
        error?.let { Text(it, style = ThemeText.detail.style, color = BaeTheme.colors.danger) }
    }
}

/** One digit's box: outlined in the secondary text colour, the accent while [ringed], red while [wrong]. */
@Composable
private fun PinCell(
    digit: Char?,
    ringed: Boolean,
    wrong: Boolean,
) {
    val (width, color) =
        when {
            wrong -> RING to BaeTheme.colors.danger
            ringed -> RING to MaterialTheme.colorScheme.primary
            else -> EDGE to MaterialTheme.colorScheme.onSurfaceVariant
        }
    Box(
        modifier = Modifier.size(CELL_WIDTH, CELL_HEIGHT).border(width, color, CELL_CORNER),
        contentAlignment = Alignment.Center,
    ) {
        Text(digit?.toString().orEmpty(), style = cellDigit, color = MaterialTheme.colorScheme.onSurface)
    }
}
