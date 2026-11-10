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
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.ComposerDetailRowsPageStore
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.metadataText
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSize
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.CoverImage
import fm.bae.app.ui.components.ErrorNotice
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.retryAction
import fm.bae.app.ui.playback.NowPlayingBar
import kotlinx.coroutines.flow.distinctUntilChanged
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerDetailItem
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerHeading
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.BridgeWorkSummary

@Composable
internal fun ComposerDetailScreen(
    session: OpenLibrary,
    artistId: String,
    onBack: () -> Unit,
    onSelectWork: (String) -> Unit,
    onSelectAlbum: (String, String) -> Unit,
) {
    val query by session.libraryQueries.composer.state
        .collectAsState()
    val detail = query.value
    val rows = session.browserPages.composerRows
    DisposableEffect(artistId, session) {
        session.libraryQueries.composer.activate(artistId)
        rows.activate(artistId)
        onDispose {
            session.libraryQueries.composer.deactivate(artistId)
            rows.deactivate(artistId)
        }
    }
    val loadError =
        when {
            query.error != null -> stringResource(R.string.composer_detail_load_failed)
            query.delivered && detail == null -> stringResource(R.string.composer_detail_not_found)
            else -> null
        }
    Column(modifier = Modifier.fillMaxSize()) {
        LibraryDetailTopBar(onBack = onBack)
        val loaded = detail
        val error = loadError
        when {
            error != null && loaded == null -> {
                ErrorText(
                    message = error,
                    modifier = Modifier.padding(ThemeSpace.page),
                )
            }

            // The detail is a local read; nothing shows until it lands.
            loaded == null -> {}

            else -> {
                error?.let {
                    ErrorText(
                        message = it,
                        modifier = Modifier.padding(ThemeSpace.related),
                    )
                }
                ComposerDetailContent(
                    detail = loaded,
                    rows = rows,
                    onSelectWork = onSelectWork,
                    onSelectAlbum = onSelectAlbum,
                )
            }
        }
        NowPlayingBar(session = session)
    }
}

/**
 * The composer, then its page's rows in core's order, a window at a time: each part of the page
 * opens with its heading, and a work listed as a part is indented under the work heading its
 * group.
 */
@Composable
private fun ComposerDetailContent(
    detail: BridgeComposerDetail,
    rows: ComposerDetailRowsPageStore,
    onSelectWork: (String) -> Unit,
    onSelectAlbum: (String, String) -> Unit,
) {
    val listState = rememberLazyListState()
    LaunchedEffect(rows, listState) {
        snapshotFlow {
            // The composer's own row comes first; the page's rows follow it.
            val visible = listState.layoutInfo.visibleItemsInfo
            ((visible.firstOrNull()?.index ?: 0) - 1).coerceAtLeast(0) to
                ((visible.lastOrNull()?.index ?: 0) - 1).coerceAtLeast(0)
        }.distinctUntilChanged().collect { (first, last) ->
            rows.reportVisibleRange(first, last)
        }
    }
    Column(modifier = Modifier.fillMaxSize()) {
        rows.error?.let { failure ->
            ErrorNotice(
                error = failure.error,
                modifier = Modifier.padding(horizontal = ThemeSpace.edge),
                action = failure.error.retryAction(onClick = failure.onRetry),
                look = NoticeLook.QUIET,
            )
        }
        LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
            item {
                ComposerSummaryRow(
                    composer = detail.composer,
                    onClick = null,
                )
            }
            items(
                count = rows.totalCount,
                key = { index -> rows.rows[index]?.id ?: "composer-row-slot-$index" },
            ) { index ->
                rows.rows[index]?.let { row ->
                    ComposerDetailRowContent(
                        row = row,
                        onSelectWork = onSelectWork,
                        onSelectAlbum = onSelectAlbum,
                    )
                }
            }
        }
    }
}

/** One row of a composer's page, under the heading it opens when it opens one. */
@Composable
private fun ComposerDetailRowContent(
    row: BridgeComposerDetailRow,
    onSelectWork: (String) -> Unit,
    onSelectAlbum: (String, String) -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth()) {
        row.heading?.let { LibrarySectionHeader(stringResource(it.titleRes)) }
        when (val item = row.item) {
            is BridgeComposerDetailItem.Work -> {
                WorkSummaryRow(
                    work = item.work,
                    part = item.part,
                    onClick = { onSelectWork(item.work.workId) },
                )
            }

            is BridgeComposerDetailItem.ReleaseCredit -> {
                Text(
                    text = item.credit.albumTitle,
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .clickable { onSelectAlbum(item.credit.albumId, item.credit.releaseId) }
                            .padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
                    style = ThemeText.rowTitle.style,
                    maxLines = 1,
                )
            }

            is BridgeComposerDetailItem.TrackCredit -> {
                TwoLineText(title = item.credit.trackTitle, subtitle = item.credit.albumTitle)
            }
        }
    }
}

/** The words of the heading over a part of a composer's page. */
private val BridgeComposerHeading.titleRes: Int
    get() =
        when (this) {
            BridgeComposerHeading.WORKS -> R.string.search_section_works
            BridgeComposerHeading.CREDITS -> R.string.search_section_credits
            BridgeComposerHeading.RECORDINGS -> R.string.search_section_recordings
        }

@Composable
internal fun WorkDetailScreen(
    session: OpenLibrary,
    workId: String,
    onBack: () -> Unit,
    onSelectWork: (String) -> Unit,
    onSelectAlbum: (String, String) -> Unit,
) {
    val query by session.libraryQueries.work.state
        .collectAsState()
    val detail = query.value
    DisposableEffect(workId, session) {
        session.libraryQueries.work.activate(workId)
        onDispose {
            session.libraryQueries.work.deactivate(workId)
        }
    }
    val loadError =
        when {
            query.error != null -> stringResource(R.string.work_detail_load_failed)
            query.delivered && detail == null -> stringResource(R.string.work_detail_not_found)
            else -> null
        }
    Column(modifier = Modifier.fillMaxSize()) {
        LibraryDetailTopBar(onBack = onBack)
        val loaded = detail
        val error = loadError
        when {
            error != null && loaded == null -> {
                ErrorText(
                    message = error,
                    modifier = Modifier.padding(ThemeSpace.page),
                )
            }

            // The detail is a local read; nothing shows until it lands.
            loaded == null -> {}

            else -> {
                error?.let {
                    ErrorText(
                        message = it,
                        modifier = Modifier.padding(ThemeSpace.related),
                    )
                }
                WorkDetailContent(
                    detail = loaded,
                    onSelectWork = onSelectWork,
                    onSelectAlbum = onSelectAlbum,
                )
            }
        }
        NowPlayingBar(session = session)
    }
}

@Composable
private fun WorkDetailContent(
    detail: BridgeWorkDetail,
    onSelectWork: (String) -> Unit,
    onSelectAlbum: (String, String) -> Unit,
) {
    val context = androidx.compose.ui.platform.LocalContext.current
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        item { WorkSummaryRow(work = detail.work, part = false, onClick = null) }
        if (detail.childWorks.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_works)) }
            items(detail.childWorks, key = { it.workId }) { work ->
                WorkSummaryRow(
                    work = work,
                    part = false,
                    onClick = { onSelectWork(work.workId) },
                )
            }
        }
        if (detail.releases.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_releases)) }
            items(detail.releases, key = { it.releaseId }) { release ->
                Row(
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .clickable { onSelectAlbum(release.albumId, release.releaseId) }
                            .padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CoverImage(
                        cover = release.cover,
                        cornerRadius = ThemeRadius.artwork,
                        iconPadding = ThemeSpace.group,
                        modifier = Modifier.size(ThemeSize.rowArtwork),
                        contentDescription = release.albumTitle,
                    )
                    Spacer(modifier = Modifier.width(ThemeSpace.group))
                    TwoLineText(title = release.albumTitle, subtitle = release.metadataText(context))
                }
            }
        }
        if (detail.tracks.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_recordings)) }
            items(detail.tracks, key = { it.trackId }) { track ->
                TwoLineText(title = track.trackTitle, subtitle = track.albumTitle)
            }
        }
    }
}

/** A work's row; [part] indents it under the work heading its group on a composer's page. */
@Composable
private fun WorkSummaryRow(
    work: BridgeWorkSummary,
    part: Boolean,
    onClick: (() -> Unit)?,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
                .padding(start = if (part) ThemeSpace.edge else 0.dp)
                .padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            cover = work.representativeCover,
            cornerRadius = ThemeRadius.artwork,
            iconPadding = ThemeSpace.group,
            modifier = Modifier.size(ThemeSize.rowArtwork),
            contentDescription = work.title,
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        TwoLineText(title = work.title, subtitle = work.composerNames)
    }
}

@Composable
internal fun TwoLineText(
    title: String,
    subtitle: String?,
) {
    Column(modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related)) {
        Text(text = title, style = ThemeText.rowTitle.style, maxLines = 1)
        if (!subtitle.isNullOrBlank()) {
            Text(
                text = subtitle,
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

@Composable
internal fun LibrarySectionHeader(title: String) {
    Text(
        text = title,
        style = ThemeText.heading.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
    )
}

@Preview(showBackground = true)
@Composable
private fun WorkSummaryRowPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            WorkSummaryRow(work = PreviewData.workSummary(), part = false, onClick = {})
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun LibrarySectionHeaderPreview() {
    BaeTheme {
        LibrarySectionHeader(title = "Works")
    }
}
