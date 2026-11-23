package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.appearance.ThemeSpace

/** The album page below its bar: the page's [content], blocks in a column [ThemeSpace.edge] in from the edges. */
@Composable
internal fun AndroidAlbumDetailContent(content: LazyListScope.() -> Unit) {
    LazyColumn(modifier = Modifier.fillMaxSize().padding(ThemeSpace.edge), content = content)
}

/** One block of the page: the board spaces them 8 dp apart, while the track rows below touch. */
internal fun LazyListScope.block(content: @Composable () -> Unit) {
    item { Box(modifier = Modifier.padding(bottom = ThemeSpace.related)) { content() } }
}
