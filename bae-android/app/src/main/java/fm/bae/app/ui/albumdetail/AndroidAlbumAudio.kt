package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/** The release's audio line, as bae read it from its files, in the tertiary colour at the foot of the album page. */
@Composable
internal fun AndroidAlbumAudio(text: String) {
    Text(
        text = text,
        style = ThemeText.detail.style,
        color = BaeTheme.colors.textTertiary,
        modifier = Modifier.padding(vertical = ThemeSpace.related),
    )
}
