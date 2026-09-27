package fm.bae.app.ui.components

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import fm.bae.app.ui.appearance.ThemeText

/** The small capitalised label that names a section, a group or a column. */
@Composable
fun Eyebrow(
    text: String,
    modifier: Modifier = Modifier,
) {
    val role = ThemeText.eyebrow
    val locale = LocalConfiguration.current.locales[0]
    Text(
        text = if (role.uppercase) text.uppercase(locale) else text,
        style = role.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = modifier,
    )
}
