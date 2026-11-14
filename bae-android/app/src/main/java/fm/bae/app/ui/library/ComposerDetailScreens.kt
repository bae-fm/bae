package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.ComposerDetailRowsPageStore
import fm.bae.app.displayError
import fm.bae.app.metadataText
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidLibraryRow
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.retryAction
import fm.bae.app.ui.playback.AndroidNowPlayingBarContainer
import kotlinx.coroutines.flow.distinctUntilChanged
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerDetailItem
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerHeading
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeWorkDetail

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
    AndroidDetailScreen(
        onBack = onBack,
        nowPlayingBar = { AndroidNowPlayingBarContainer(session = session) },
    ) {
        val loaded = detail
        val error = loadError
        when {
            error != null && loaded == null -> {
                ComposerLoadFailure(error, query.error, session.libraryQueries.composer::retry)
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
    }
}

/** A composer page that did not load: the centred notice when core failed, else the plain line. */
@Composable
private fun ComposerLoadFailure(
    title: String,
    failure: BridgeException?,
    onRetry: () -> Unit,
) {
    if (failure == null) {
        ErrorText(message = title, modifier = Modifier.padding(ThemeSpace.page))
        return
    }
    val shown = LocalContext.current.displayError(failure)
    AndroidNotice(
        title = title,
        advice = null,
        error = shown,
        look = NoticeLook.CENTERED,
        action = shown.retryAction(onClick = onRetry),
    )
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
    val context = LocalContext.current
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
            AndroidNotice(
                error = failure.error,
                modifier = Modifier.padding(horizontal = ThemeSpace.edge),
                action = failure.error.retryAction(onClick = failure.onRetry),
                look = NoticeLook.QUIET,
            )
        }
        LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
            item {
                AndroidLibraryRow(
                    title = detail.composer.name,
                    subtitle = context.coreString(detail.composer.countLine),
                    art = detail.composer.image,
                    missing = detail.composer.image == null,
                )
            }
            items(
                count = rows.totalCount,
                key = { index -> rows.rowAt(index)?.id ?: "composer-row-slot-$index" },
            ) { index ->
                rows.rowAt(index)?.let { row ->
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
        row.heading?.let { AndroidSearchSection(stringResource(it.titleRes)) }
        when (val item = row.item) {
            is BridgeComposerDetailItem.Work -> {
                AndroidLibraryRow(
                    title = item.work.title,
                    subtitle = item.work.composerNames,
                    art = item.work.representativeCover,
                    part = item.part,
                    inset = true,
                    onClick = { onSelectWork(item.work.workId) },
                )
            }

            is BridgeComposerDetailItem.ReleaseCredit -> {
                AndroidLibraryRow(
                    title = item.credit.albumTitle,
                    noArt = true,
                    onClick = { onSelectAlbum(item.credit.albumId, item.credit.releaseId) },
                )
            }

            is BridgeComposerDetailItem.TrackCredit -> {
                AndroidLibraryRow(
                    title = item.credit.trackTitle,
                    subtitle = item.credit.albumTitle,
                    noArt = true,
                )
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
    AndroidDetailScreen(
        onBack = onBack,
        nowPlayingBar = { AndroidNowPlayingBarContainer(session = session) },
    ) {
        val loaded = detail
        val error = loadError
        when {
            error != null && loaded == null -> {
                ComposerLoadFailure(error, query.error, session.libraryQueries.composer::retry)
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
    }
}

@Composable
private fun WorkDetailContent(
    detail: BridgeWorkDetail,
    onSelectWork: (String) -> Unit,
    onSelectAlbum: (String, String) -> Unit,
) {
    val context = LocalContext.current
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        item {
            AndroidLibraryRow(
                title = detail.work.title,
                subtitle = detail.work.composerNames,
                art = detail.work.representativeCover,
                inset = true,
            )
        }
        if (detail.childWorks.isNotEmpty()) {
            item { AndroidSearchSection(stringResource(R.string.search_section_works)) }
            items(detail.childWorks, key = { it.workId }) { work ->
                AndroidLibraryRow(
                    title = work.title,
                    subtitle = work.composerNames,
                    art = work.representativeCover,
                    inset = true,
                    onClick = { onSelectWork(work.workId) },
                )
            }
        }
        if (detail.releases.isNotEmpty()) {
            item { AndroidSearchSection(stringResource(R.string.search_section_releases)) }
            items(detail.releases, key = { it.releaseId }) { release ->
                AndroidLibraryRow(
                    title = release.albumTitle,
                    subtitle = release.metadataText(context),
                    art = release.cover,
                    inset = true,
                    onClick = { onSelectAlbum(release.albumId, release.releaseId) },
                )
            }
        }
        if (detail.tracks.isNotEmpty()) {
            item { AndroidSearchSection(stringResource(R.string.search_section_recordings)) }
            items(detail.tracks, key = { it.trackId }) { track ->
                AndroidLibraryRow(title = track.trackTitle, subtitle = track.albumTitle, noArt = true)
            }
        }
    }
}

@Composable
internal fun AndroidSearchSection(title: String) {
    Text(
        text = title,
        style = ThemeText.heading.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
    )
}
