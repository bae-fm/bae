package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.countText
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * The gallery's caption on the dark backdrop: the shown image's [label] and, when there are several images, its
 * [page] of [count].
 */
@Composable
internal fun AndroidGalleryCaption(
    label: String,
    page: Int,
    count: UInt,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier =
            modifier
                .windowInsetsPadding(
                    WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal),
                ).padding(ThemeSpace.edge),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(text = label, style = ThemeText.body.style, color = BaeTheme.colors.onFill)
        if (count > 1u) {
            Text(
                text =
                    stringResource(
                        R.string.gallery_page_counter,
                        LocalContext.current.countText(page),
                        LocalContext.current.countText(count),
                    ),
                style = ThemeText.body.style,
                color = BaeTheme.colors.onFillSecondary,
            )
        }
    }
}
