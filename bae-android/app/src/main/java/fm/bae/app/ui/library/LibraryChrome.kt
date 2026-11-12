package fm.bae.app.ui.library

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.data.AlbumPageStore
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidInlineIconButton

/** A later page of the album grid that failed to load, above the rows that did, with Retry. */
@Composable
internal fun AndroidErrorBannerContainer(page: AlbumPageStore) {
    val appendError = (if (page.rows.isNotEmpty()) page.error else null) ?: return
    AndroidErrorBanner(
        title = stringResource(R.string.albums_append_failed),
        error = appendError.error,
        onRetry = appendError.onRetry,
    )
}

/** The search field's height; its pill shape is half of it. */
private val SEARCH_FIELD_HEIGHT = 56.dp

@Composable
internal fun LibrarySearchBar(
    query: String,
    onQueryChange: (String) -> Unit,
    onClose: () -> Unit,
) {
    val focusRequester = remember { FocusRequester() }
    LaunchedEffect(Unit) { focusRequester.requestFocus() }
    Surface(color = MaterialTheme.colorScheme.surface) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(ThemeSpace.related),
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onClose) {
                Icon(
                    imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                    contentDescription = stringResource(R.string.close_search),
                )
            }
            BasicTextField(
                value = query,
                onValueChange = onQueryChange,
                modifier = Modifier.weight(1f).focusRequester(focusRequester),
                textStyle = ThemeText.androidBodyLarge.style.copy(color = MaterialTheme.colorScheme.onSurface),
                singleLine = true,
                cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                decorationBox = { field ->
                    Row(
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .height(SEARCH_FIELD_HEIGHT)
                                .background(BaeTheme.surfaces.elevated, CircleShape)
                                .padding(horizontal = ThemeSpace.edge),
                        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.edge),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(modifier = Modifier.weight(1f)) {
                            if (query.isEmpty()) {
                                Text(
                                    stringResource(R.string.search_placeholder),
                                    style = ThemeText.androidBodyLarge.style,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                            field()
                        }
                        if (query.isNotEmpty()) {
                            AndroidInlineIconButton(
                                icon = Icons.Filled.Close,
                                contentDescription = stringResource(R.string.clear_search),
                                onClick = { onQueryChange("") },
                            )
                        }
                    }
                },
            )
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun LibrarySearchBarPreview() {
    PreviewTheme {
        LibrarySearchBar(query = "search text", onQueryChange = {}, onClose = {})
    }
}
