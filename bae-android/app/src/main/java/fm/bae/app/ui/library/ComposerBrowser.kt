package fm.bae.app.ui.library

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.ComposerPageStore
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSize
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.CoverImage
import kotlinx.coroutines.flow.distinctUntilChanged
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary

@Composable
internal fun rememberComposerPage(
    session: OpenLibrary,
    sortCriterion: BridgeComposerSortCriterion,
): ComposerPageStore {
    val page = session.browserPages.composers
    DisposableEffect(page, sortCriterion) {
        page.activate(sortCriterion)
        onDispose(page::deactivate)
    }
    return page
}

@Composable
internal fun ComposerListContent(
    session: OpenLibrary,
    page: ComposerPageStore,
    listState: LazyListState,
    onSelectComposer: (String) -> Unit,
) {
    LibraryPageContent(
        session = session,
        page = page,
        emptyMessage = stringResource(R.string.library_empty_composers),
    ) {
        LaunchedEffect(page, listState) {
            snapshotFlow {
                val visible = listState.layoutInfo.visibleItemsInfo
                (visible.firstOrNull()?.index ?: 0) to (visible.lastOrNull()?.index ?: 0)
            }.distinctUntilChanged().collect { (first, last) ->
                page.reportVisibleRange(first, last)
            }
        }
        LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
            items(
                count = page.totalCount,
                key = { index -> page.rows[index]?.artistId ?: "composer-slot-$index" },
            ) { index ->
                page.rows[index]?.let { composer ->
                    ComposerSummaryRow(
                        composer = composer,
                        onClick = { onSelectComposer(composer.artistId) },
                    )
                }
            }
        }
    }
}

@Composable
internal fun ComposerSummaryRow(
    composer: BridgeComposerSummary,
    onClick: (() -> Unit)?,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
                .padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            cover = composer.image,
            cornerRadius = ThemeRadius.artwork,
            iconPadding = ThemeSpace.group,
            modifier = Modifier.size(ThemeSize.rowArtwork),
            contentDescription = composer.name,
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = composer.name,
                style = ThemeText.rowTitle.style,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = pluralStringResource(R.plurals.work_count, composer.workCount.toInt(), composer.workCount),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun ComposerSummaryRowPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            ComposerSummaryRow(composer = PreviewData.composerSummary(), onClick = {})
        }
    }
}
