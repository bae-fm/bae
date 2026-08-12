package fm.bae.app.ui.library

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.CoverImage
import fm.bae.app.ui.playback.NowPlayingBar
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.BridgeWorkReleaseSummary
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
    val appContext = androidx.compose.ui.platform.LocalContext.current
    DisposableEffect(artistId, session) {
        session.libraryQueries.composer.activate(artistId)
        onDispose {
            session.libraryQueries.composer.deactivate(artistId)
        }
    }
    val loadError =
        when {
            query.error != null -> appContext.getString(R.string.composer_detail_load_failed)
            query.delivered && detail == null -> appContext.getString(R.string.composer_detail_not_found)
            else -> null
        }
    Column(modifier = Modifier.fillMaxSize()) {
        LibraryDetailTopBar(onBack = onBack)
        val loaded = detail
        val error = loadError
        when {
            error != null && loaded == null -> {
                Text(
                    text = error,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(32.dp),
                )
            }

            loaded == null -> {
                Box(modifier = Modifier.fillMaxSize()) {
                    CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
                }
            }

            else -> {
                error?.let {
                    Text(text = it, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(8.dp))
                }
                ComposerDetailContent(
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
private fun ComposerDetailContent(
    detail: BridgeComposerDetail,
    onSelectWork: (String) -> Unit,
    onSelectAlbum: (String, String) -> Unit,
) {
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        item {
            ComposerSummaryRow(
                composer = detail.composer,
                onClick = null,
            )
        }
        if (detail.workGroups.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_works)) }
            detail.workGroups.forEach { group ->
                group.parent?.let { parent ->
                    item(key = "parent:${parent.workId}") {
                        WorkSummaryRow(
                            work = parent,
                            onClick = { onSelectWork(parent.workId) },
                        )
                    }
                }
                items(group.works, key = { "work:${it.workId}" }) { work ->
                    WorkSummaryRow(
                        work = work,
                        onClick = { onSelectWork(work.workId) },
                    )
                }
            }
        }
        if (detail.unlinkedReleaseRoles.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_credits)) }
            items(detail.unlinkedReleaseRoles, key = { it.releaseId }) { role ->
                Text(
                    text = role.albumTitle,
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .clickable { onSelectAlbum(role.albumId, role.releaseId) }
                            .padding(horizontal = 16.dp, vertical = 8.dp),
                    style = MaterialTheme.typography.bodyLarge,
                    maxLines = 1,
                )
            }
        }
        if (detail.unlinkedTrackRoles.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_recordings)) }
            items(detail.unlinkedTrackRoles, key = { it.trackId }) { role ->
                TwoLineText(title = role.trackTitle, subtitle = role.albumTitle)
            }
        }
    }
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
    val appContext = androidx.compose.ui.platform.LocalContext.current
    DisposableEffect(workId, session) {
        session.libraryQueries.work.activate(workId)
        onDispose {
            session.libraryQueries.work.deactivate(workId)
        }
    }
    val loadError =
        when {
            query.error != null -> appContext.getString(R.string.work_detail_load_failed)
            query.delivered && detail == null -> appContext.getString(R.string.work_detail_not_found)
            else -> null
        }
    Column(modifier = Modifier.fillMaxSize()) {
        LibraryDetailTopBar(onBack = onBack)
        val loaded = detail
        val error = loadError
        when {
            error != null && loaded == null -> {
                Text(
                    text = error,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(32.dp),
                )
            }

            loaded == null -> {
                Box(modifier = Modifier.fillMaxSize()) {
                    CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
                }
            }

            else -> {
                error?.let {
                    Text(text = it, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(8.dp))
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
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        item { WorkSummaryRow(work = detail.work, onClick = null) }
        if (detail.childWorks.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_works)) }
            items(detail.childWorks, key = { it.workId }) { work ->
                WorkSummaryRow(
                    work = work,
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
                            .padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CoverImage(
                        cover = release.cover,
                        cornerRadius = 6.dp,
                        iconPadding = 12.dp,
                        modifier = Modifier.size(48.dp),
                        contentDescription = release.albumTitle,
                    )
                    Spacer(modifier = Modifier.width(12.dp))
                    TwoLineText(title = release.albumTitle, subtitle = workReleaseMetadata(release))
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

private fun workReleaseMetadata(release: BridgeWorkReleaseSummary): String =
    if (release.format.isNullOrEmpty()) {
        check(release.displayName.isNotEmpty()) {
            "work release display name is empty for ${release.releaseId}"
        }
        release.displayName
    } else {
        "${release.displayName} · ${release.format}"
    }

@Composable
private fun WorkSummaryRow(
    work: BridgeWorkSummary,
    onClick: (() -> Unit)?,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
                .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            cover = work.representativeCover,
            cornerRadius = 6.dp,
            iconPadding = 12.dp,
            modifier = Modifier.size(48.dp),
            contentDescription = work.title,
        )
        Spacer(modifier = Modifier.width(12.dp))
        TwoLineText(title = work.title, subtitle = work.composerNames)
    }
}

@Composable
internal fun TwoLineText(
    title: String,
    subtitle: String?,
) {
    Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp)) {
        Text(text = title, style = MaterialTheme.typography.bodyLarge, maxLines = 1)
        if (!subtitle.isNullOrBlank()) {
            Text(
                text = subtitle,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

@Composable
internal fun LibraryDetailTopBar(onBack: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
            }
            Text(text = "bae", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        }
    }
}

@Composable
internal fun LibrarySectionHeader(title: String) {
    Text(
        text = title,
        style = MaterialTheme.typography.titleSmall,
        fontWeight = FontWeight.Bold,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
    )
}

@Preview(showBackground = true)
@Composable
private fun WorkSummaryRowPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            WorkSummaryRow(work = PreviewData.workSummary(), onClick = {})
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
