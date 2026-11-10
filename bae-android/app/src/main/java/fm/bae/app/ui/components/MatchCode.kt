package fm.bae.app.ui.components

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.sp
import fm.bae.app.ui.appearance.ThemeSpace

private val dialogDigits =
    TextStyle(fontSize = 32.sp, lineHeight = 40.sp, fontWeight = FontWeight.Medium, letterSpacing = 2.sp)

/**
 * The six digits two devices both show while one lets the other in, large and
 * centred so the person can read them aloud and check they match, in the size
 * a dialog uses.
 */
@Composable
fun MatchCode(
    code: String,
    modifier: Modifier = Modifier,
) {
    SelectionContainer {
        Text(
            text = code,
            style = dialogDigits,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
            modifier = modifier.fillMaxWidth().padding(top = ThemeSpace.related),
        )
    }
}
