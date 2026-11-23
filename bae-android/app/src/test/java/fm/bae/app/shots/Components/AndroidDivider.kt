package fm.bae.app.shots

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidDivider
import org.junit.Test

/** The component shots of `AndroidDivider`, one for each row of its preview. */
class AndroidDividerShots : BoardShot() {
    @Test
    fun androidComponentDividerDivider() =
        component("android.component.divider.divider") {
            Column {
                FineLine("Above")
                AndroidDivider()
                FineLine("Below")
            }
        }
}

/** A line of the preview's detail text, in secondary colour, with the board's 8dp above and below it. */
@Composable
private fun FineLine(text: String) {
    Text(
        text,
        style = ThemeText.detail.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(vertical = ThemeSpace.related),
    )
}
