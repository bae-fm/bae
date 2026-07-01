package fm.bae.app.ui

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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.BridgeWorkReleaseSummary
import uniffi.bae_bridge.BridgeWorkSummary

private const val TAG = "bae.ComposerDetailScreens"
private val logger = BaeLogger(TAG)

@Composable
internal fun ComposerDetailScreen(
    session: OpenLibrary,
    artistId: String,
    onBack: () -> Unit,
    onSelectWork: (String) -> Unit,
    onSelectAlbum: (String) -> Unit,
) {
    var detail by remember(artistId) { mutableStateOf<BridgeComposerDetail?>(null) }
    var loadError by remember(artistId) { mutableStateOf<String?>(null) }
    val appContext = androidx.compose.ui.platform.LocalContext.current
    LaunchedEffect(artistId) {
        loadError = null
        try {
            detail = withContext(Dispatchers.IO) { session.library.composerDetail(artistId) }
            if (detail == null) {
                loadError = appContext.getString(R.string.composer_detail_not_found)
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed to load composer detail $artistId", e)
            loadError = appContext.getString(R.string.composer_detail_load_failed)
        }
    }
    Column(modifier = Modifier.fillMaxSize()) {
        LibraryDetailTopBar(onBack = onBack)
        val loaded = detail
        val error = loadError
        when {
            error != null -> {
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
                ComposerDetailContent(
                    detail = loaded,
                    loadImage = session.library::imageBytes,
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
    loadImage: suspend (imageId: String) -> ByteArray?,
    onSelectWork: (String) -> Unit,
    onSelectAlbum: (String) -> Unit,
) {
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        item {
            ComposerSummaryRow(
                composer = detail.composer,
                loadImage = loadImage,
                onClick = {},
            )
        }
        if (detail.workGroups.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_works)) }
            detail.workGroups.forEach { group ->
                group.parent?.let { parent ->
                    item(key = "parent:${parent.workId}") {
                        WorkSummaryRow(
                            work = parent,
                            loadImage = loadImage,
                            onClick = { onSelectWork(parent.workId) },
                        )
                    }
                }
                items(group.works, key = { "work:${it.workId}" }) { work ->
                    WorkSummaryRow(
                        work = work,
                        loadImage = loadImage,
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
                            .clickable { onSelectAlbum(role.albumId) }
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
    var detail by remember(workId) { mutableStateOf<BridgeWorkDetail?>(null) }
    var loadError by remember(workId) { mutableStateOf<String?>(null) }
    val appContext = androidx.compose.ui.platform.LocalContext.current
    LaunchedEffect(workId) {
        loadError = null
        try {
            detail = withContext(Dispatchers.IO) { session.library.workDetail(workId) }
            if (detail == null) {
                loadError = appContext.getString(R.string.work_detail_not_found)
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed to load work detail $workId", e)
            loadError = appContext.getString(R.string.work_detail_load_failed)
        }
    }
    Column(modifier = Modifier.fillMaxSize()) {
        LibraryDetailTopBar(onBack = onBack)
        val loaded = detail
        val error = loadError
        when {
            error != null -> {
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
                WorkDetailContent(
                    detail = loaded,
                    loadImage = session.library::imageBytes,
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
    loadImage: suspend (imageId: String) -> ByteArray?,
    onSelectWork: (String) -> Unit,
    onSelectAlbum: (String, String) -> Unit,
) {
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        item { WorkSummaryRow(work = detail.work, loadImage = loadImage, onClick = {}) }
        if (detail.childWorks.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_works)) }
            items(detail.childWorks, key = { it.workId }) { work ->
                WorkSummaryRow(
                    work = work,
                    loadImage = loadImage,
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
                        coverId = release.cover?.id,
                        coverVersion = release.cover?.version,
                        loadImage = loadImage,
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
    listOfNotNull(release.displayName, release.format)
        .filter { it.isNotEmpty() }
        .joinToString(" · ")

@Composable
private fun WorkSummaryRow(
    work: BridgeWorkSummary,
    loadImage: suspend (imageId: String) -> ByteArray?,
    onClick: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(onClick = onClick)
                .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            coverId = work.representativeCover?.id,
            coverVersion = work.representativeCover?.version,
            loadImage = loadImage,
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
private fun TwoLineText(
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
private fun LibraryDetailTopBar(onBack: () -> Unit) {
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
private fun LibrarySectionHeader(title: String) {
    Text(
        text = title,
        style = MaterialTheme.typography.titleSmall,
        fontWeight = FontWeight.Bold,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
    )
}
