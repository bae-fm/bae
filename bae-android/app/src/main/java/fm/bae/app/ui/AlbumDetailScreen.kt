package fm.bae.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.PlaylistPlay
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Shuffle
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.durationClockText
import fm.bae.app.durationUnitsText
import fm.bae.app.runLoggedBridgeCommand
import fm.bae.app.sideHeaderText
import fm.bae.app.text
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeGallerySource
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeRelease

private const val TAG = "bae.AlbumDetailScreen"
private val logger = BaeLogger(TAG)

private data class AlbumPlaybackState(
    val currentTrackId: String?,
    val isPlaying: Boolean,
)

private data class AlbumDetailCallbacks(
    val fetchGalleryBytes: suspend (releaseId: String, source: BridgeGallerySource) -> ByteArray,
    val loadCoverImage: suspend (imageId: String) -> ByteArray?,
    val onSelectRelease: (String) -> Unit,
    val onTogglePlayPause: () -> Unit,
    val onPlayTrackAt: (Int) -> Unit,
    val onPlayRelease: () -> Unit,
    val onShuffleRelease: () -> Unit,
    val onPlayReleaseNext: () -> Unit,
    val onAddReleaseToQueue: () -> Unit,
    val onPlayTrackNext: (String) -> Unit,
    val onAddTrackToQueue: (String) -> Unit,
)

/**
 * Album detail: header, a release picker when the album has more than one
 * release, and the selected release's track list grouped by side. Tapping a
 * track plays the release starting at that track. A now-playing bar sits at
 * the bottom.
 */
@Composable
fun AlbumDetailScreen(
    session: OpenLibrary,
    albumId: String,
    initialReleaseId: String? = null,
    onBack: () -> Unit,
) {
    val details by session.libraryStore.albumDetails.collectAsState()
    val detail = details[albumId]
    val nowPlaying by session.playback.nowPlaying.collectAsState()
    val isPlaying by session.playback.isPlaying.collectAsState()
    var selectedReleaseId by remember(albumId, initialReleaseId) { mutableStateOf(initialReleaseId) }
    var retryToken by remember(albumId) { mutableStateOf(0) }
    var loadError by remember(albumId) { mutableStateOf<String?>(null) }
    val appContext = LocalContext.current

    LaunchedEffect(albumId, retryToken) {
        loadError = null
        if (session.libraryStore.albumDetail(albumId) == null) {
            try {
                val loaded = withContext(Dispatchers.IO) { session.library.albumDetail(albumId) }
                session.libraryStore.seedAlbumDetail(loaded)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logger.error("Failed to load album detail $albumId", e)
                loadError = e.message ?: appContext.getString(R.string.album_load_failed)
            }
        }
    }

    LaunchedEffect(detail) {
        if (selectedReleaseId == null && detail != null) {
            val primary = detail.album.primaryReleaseId
            selectedReleaseId =
                if (detail.releases.any { it.id == primary }) {
                    primary
                } else {
                    detail.releases.firstOrNull()?.id
                }
        }
    }

    Column(modifier = Modifier.fillMaxSize()) {
        AlbumDetailTopBar(onBack = onBack)
        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            val loaded = detail
            if (loaded == null) {
                AlbumDetailLoadingBox(loadError, onRetry = { retryToken++ })
            } else {
                val release = loaded.releases.firstOrNull { it.id == selectedReleaseId }
                AlbumDetailContent(
                    session = session,
                    detail = loaded,
                    selectedRelease = release,
                    cover = release?.cover,
                    playback = AlbumPlaybackState(nowPlaying?.trackId, isPlaying),
                    callbacks = buildAlbumDetailCallbacks(session, release) { selectedReleaseId = it },
                )
            }
        }
        NowPlayingBar(session = session)
    }
}

@Composable
private fun BoxScope.AlbumDetailLoadingBox(
    loadError: String?,
    onRetry: () -> Unit,
) {
    if (loadError != null) {
        Column(
            modifier = Modifier.align(Alignment.Center).padding(32.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(text = loadError, color = MaterialTheme.colorScheme.error)
            TextButton(onClick = onRetry) { Text(stringResource(R.string.retry)) }
        }
    } else {
        CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
    }
}

@Composable
private fun AlbumDetailTopBar(onBack: () -> Unit) {
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

private fun buildAlbumDetailCallbacks(
    session: OpenLibrary,
    release: BridgeRelease?,
    onSelectRelease: (String) -> Unit,
): AlbumDetailCallbacks {
    fun runPlayReleaseCommand(
        operation: (BridgeRelease) -> String,
        startIndex: UInt?,
        shuffled: Boolean,
    ) {
        release?.let {
            runLoggedBridgeCommand(logger, operation(it)) {
                session.appHandle.playRelease(it.id, startIndex, shuffled)
            }
        }
    }

    fun runReleaseTransportCommand(
        operation: String,
        command: (String) -> Unit,
    ) {
        release?.let {
            runLoggedBridgeCommand(logger, "$operation ${it.id}") {
                command(it.id)
            }
        }
    }

    fun runTrackTransportCommand(
        trackId: String,
        operation: String,
        command: (List<String>) -> Unit,
    ) {
        runLoggedBridgeCommand(logger, "$operation $trackId") {
            command(listOf(trackId))
        }
    }

    return AlbumDetailCallbacks(
        fetchGalleryBytes = { releaseId, source -> session.appHandle.fetchGalleryBytes(releaseId, source) },
        loadCoverImage = session.library::imageBytes,
        onSelectRelease = onSelectRelease,
        onTogglePlayPause = { session.playback.togglePlayPause() },
        // play_release is a core transport command: ask core to play the release at a track index.
        onPlayTrackAt = { index ->
            runPlayReleaseCommand(
                operation = { "playRelease ${it.id} track $index" },
                startIndex = index.toUInt(),
                shuffled = false,
            )
        },
        onPlayRelease = {
            runPlayReleaseCommand(
                operation = { "playRelease ${it.id}" },
                startIndex = null,
                shuffled = false,
            )
        },
        onShuffleRelease = {
            runPlayReleaseCommand(
                operation = { "shuffleRelease ${it.id}" },
                startIndex = null,
                shuffled = true,
            )
        },
        onPlayReleaseNext = {
            runReleaseTransportCommand("addReleaseNext", session.appHandle::addReleaseNext)
        },
        onAddReleaseToQueue = {
            runReleaseTransportCommand("addReleaseToQueue", session.appHandle::addReleaseToQueue)
        },
        onPlayTrackNext = { trackId ->
            runTrackTransportCommand(trackId, "addNext", session.appHandle::addNext)
        },
        onAddTrackToQueue = { trackId ->
            runTrackTransportCommand(trackId, "addToQueue", session.appHandle::addToQueue)
        },
    )
}

@Composable
private fun AlbumDetailContent(
    session: OpenLibrary,
    detail: BridgeAlbumDetail,
    selectedRelease: BridgeRelease?,
    cover: BridgeImageRef?,
    playback: AlbumPlaybackState,
    callbacks: AlbumDetailCallbacks,
) {
    val album = detail.album
    val context = LocalContext.current

    var showGallery by remember { mutableStateOf(false) }
    val galleryRelease = selectedRelease
    val galleryItems = galleryRelease?.galleryItems ?: emptyList()
    if (showGallery && galleryRelease != null && galleryItems.isNotEmpty()) {
        GalleryDialog(
            releaseId = galleryRelease.id,
            items = galleryItems,
            loadImage = { source -> callbacks.fetchGalleryBytes(galleryRelease.id, source) },
            onDismiss = { showGallery = false },
        )
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        item {
            AlbumDetailHeader(
                detail = detail,
                selectedRelease = selectedRelease,
                cover = cover,
                loadImage = callbacks.loadCoverImage,
                galleryItems = galleryItems,
                onShowGallery = { showGallery = true },
            )
        }

        if (detail.releases.size > 1) {
            item {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    detail.releases.forEach { rel ->
                        FilterChip(
                            selected = rel.id == selectedRelease?.id,
                            onClick = { callbacks.onSelectRelease(rel.id) },
                            label = { Text(rel.displayName, maxLines = 1) },
                        )
                    }
                }
            }
        }

        if (selectedRelease != null) {
            item { ReleaseDownloadControl(session = session, release = selectedRelease) }
            item { AlbumActionButtons(callbacks) }
            albumTrackGroups(selectedRelease, playback, callbacks, context)
        }
    }
}

@Composable
private fun AlbumDetailHeader(
    detail: BridgeAlbumDetail,
    selectedRelease: BridgeRelease?,
    cover: BridgeImageRef?,
    loadImage: suspend (imageId: String) -> ByteArray?,
    galleryItems: List<uniffi.bae_bridge.BridgeGalleryItem>,
    onShowGallery: () -> Unit,
) {
    val album = detail.album
    val context = LocalContext.current

    fun compactMeta(): String {
        val release = selectedRelease ?: return ""
        val audioFormat = release.files.firstNotNullOfOrNull { it.audioFormat }?.text(context)
        return listOfNotNull(
            release.year?.toString(),
            release.format,
            release.label,
            release.catalogNumber,
            release.country,
            audioFormat,
        ).joinToString(" · ")
    }
    Row(verticalAlignment = Alignment.Top) {
        CoverImage(
            coverId = cover?.id,
            coverVersion = cover?.version,
            loadImage = loadImage,
            cornerRadius = 6.dp,
            iconPadding = 32.dp,
            modifier =
                Modifier
                    .size(140.dp)
                    .clickable(enabled = galleryItems.isNotEmpty(), onClick = onShowGallery),
            contentDescription = album.title,
        )
        Spacer(modifier = Modifier.width(16.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(text = album.title, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
            Text(
                text = album.artistNames,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            album.year?.let {
                Text(
                    text = it.toString(),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            val meta = compactMeta()
            if (meta.isNotEmpty()) {
                Spacer(modifier = Modifier.height(4.dp))
                Text(
                    text = meta,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
private fun AlbumActionButtons(callbacks: AlbumDetailCallbacks) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = callbacks.onPlayRelease) {
                Icon(Icons.Filled.PlayArrow, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(modifier = Modifier.width(8.dp))
                Text(stringResource(R.string.play))
            }
            OutlinedButton(onClick = callbacks.onShuffleRelease) {
                Icon(Icons.Filled.Shuffle, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(modifier = Modifier.width(8.dp))
                Text(stringResource(R.string.shuffle))
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = callbacks.onPlayReleaseNext) {
                Icon(Icons.AutoMirrored.Filled.PlaylistPlay, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(modifier = Modifier.width(8.dp))
                Text(stringResource(R.string.play_next))
            }
            OutlinedButton(onClick = callbacks.onAddReleaseToQueue) {
                Icon(Icons.Filled.Add, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(modifier = Modifier.width(8.dp))
                Text(stringResource(R.string.add_to_queue))
            }
        }
    }
}

private fun androidx.compose.foundation.lazy.LazyListScope.albumTrackGroups(
    release: BridgeRelease,
    playback: AlbumPlaybackState,
    callbacks: AlbumDetailCallbacks,
    context: android.content.Context,
) {
    // Flatten groups to a release-wide track index so taps map to the ordered
    // list the player builds from the same flattening.
    var runningIndex = 0
    release.trackGroups.forEach { group ->
        val sideHeader = group.side.sideHeaderText(context)
        if (sideHeader.isNotEmpty()) {
            item {
                Text(
                    text = sideHeader,
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 8.dp),
                )
            }
        }
        val groupOffset = runningIndex
        itemsIndexed(group.tracks, key = { _, t -> t.id }) { localIndex, track ->
            val isCurrent = track.id == playback.currentTrackId
            TrackRow(
                data =
                    TrackRowData(
                        positionLabel = track.positionText,
                        title = track.title,
                        artistNames = track.displayArtist,
                        durationLabel = context.durationClockText(track.durationMs),
                        isCurrent = isCurrent,
                        isPlaying = playback.isPlaying,
                    ),
                callbacks =
                    TrackRowCallbacks(
                        onClick = {
                            if (isCurrent) {
                                callbacks.onTogglePlayPause()
                            } else {
                                callbacks.onPlayTrackAt(groupOffset + localIndex)
                            }
                        },
                        onPlayNext = { callbacks.onPlayTrackNext(track.id) },
                        onAddToQueue = { callbacks.onAddTrackToQueue(track.id) },
                    ),
            )
        }
        runningIndex += group.tracks.size
    }
    val totalDurationLabel = context.durationUnitsText(release.totalDuration)
    if (totalDurationLabel.isNotEmpty()) {
        item {
            Text(
                text = totalDurationLabel,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(top = 8.dp),
            )
        }
    }
}
