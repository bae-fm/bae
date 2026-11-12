package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import fm.bae.app.ui.appearance.ThemeSpace

private val screenDigits =
    TextStyle(fontSize = 40.sp, lineHeight = 48.sp, fontWeight = FontWeight.Medium, letterSpacing = 3.sp)
private val dialogDigits =
    TextStyle(fontSize = 32.sp, lineHeight = 40.sp, fontWeight = FontWeight.Medium, letterSpacing = 2.sp)
private val WAITING_SPINNER = 16.dp

/**
 * The six digits two devices both show while one lets the other in, large and
 * centred so the person can read them aloud and check they match. On a screen
 * they are the larger size; in a [dialog] the smaller, set a little further
 * from the line above so the code stands apart from the words that ask for the
 * check. [waiting] is the line under the digits that says what the device is
 * waiting for, beside a spinner.
 */
@Composable
fun AndroidMatchCode(
    code: String,
    modifier: Modifier = Modifier,
    dialog: Boolean = false,
    waiting: String? = null,
) {
    Column(
        modifier = modifier.fillMaxWidth().then(if (dialog) Modifier.padding(top = ThemeSpace.related) else Modifier),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.edge),
    ) {
        SelectionContainer {
            Text(
                text = code,
                style = if (dialog) dialogDigits else screenDigits,
                color = MaterialTheme.colorScheme.onSurface,
                textAlign = TextAlign.Center,
            )
        }
        waiting?.let { AndroidSpinner(text = it, size = WAITING_SPINNER) }
    }
}
