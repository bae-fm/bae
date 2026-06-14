package fm.bae.app.ui

import android.util.Log
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.PlaylistPlay
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Shuffle
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import fm.bae.app.OpenLibrary
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeRelease

private const val TAG = "bae.AlbumDetailScreen"

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
    onBack: () -> Unit,
) {
    // The store is the source of truth for detail: sync events keep it live
    // (e.g. a release's cover or pin state changing mid-view). Seed it from the
    // DB on first open when no event has populated this album yet.
    val details by session.libraryStore.albumDetails.collectAsState()
    val detail = details[albumId]
    val nowPlaying by session.playback.nowPlaying.collectAsState()
    val isPlaying by session.playback.isPlaying.collectAsState()
    var selectedReleaseId by remember { mutableStateOf<String?>(null) }
    // Bumped by Retry after a failed seed; re-runs the load below.
    var retryToken by remember(albumId) { mutableStateOf(0) }
    // Set when the seed read throws, so the spinner gives way to an error+retry
    // instead of spinning forever. Cleared on album change or retry.
    var loadError by remember(albumId) { mutableStateOf<String?>(null) }

    LaunchedEffect(albumId, retryToken) {
        loadError = null
        if (session.libraryStore.albumDetail(albumId) == null) {
            try {
                val loaded = withContext(Dispatchers.IO) { session.library.albumDetail(albumId) }
                session.libraryStore.seedAlbumDetail(loaded)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                Log.e(TAG, "Failed to load album detail $albumId", e)
                loadError = e.message ?: "Couldn't load this album."
            }
        }
    }

    // Pick the initial release once detail is available: primary, else first.
    LaunchedEffect(detail) {
        if (selectedReleaseId == null && detail != null) {
            val primary = detail.album.primaryReleaseId
            selectedReleaseId = if (detail.releases.any { it.id == primary }) {
                primary
            } else {
                detail.releases.firstOrNull()?.id
            }
        }
    }

    Column(modifier = Modifier.fillMaxSize()) {
        Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = onBack) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                }
                Text(text = "bae", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
            }
        }

        val loaded = detail
        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            if (loaded == null) {
                if (loadError != null) {
                    Column(
                        modifier = Modifier.align(Alignment.Center).padding(32.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        Text(
                            text = loadError ?: "",
                            color = MaterialTheme.colorScheme.error,
                        )
                        TextButton(onClick = { retryToken++ }) { Text("Retry") }
                    }
                } else {
                    CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
                }
            } else {
                val release = loaded.releases.firstOrNull { it.id == selectedReleaseId }
                AlbumDetailContent(
                    detail = loaded,
                    selectedRelease = release,
                    // The cover is the selected release's first gallery item; its
                    // localPath is an absolute path the bridge already resolved.
                    coverPath = release?.galleryItems?.firstOrNull()?.localPath,
                    onSelectRelease = { newId ->
                        selectedReleaseId = newId
                    },
                    currentTrackId = nowPlaying?.trackId,
                    isPlaying = isPlaying,
                    onTogglePlayPause = { session.playback.togglePlayPause() },
                    onPlayTrackAt = { index ->
                        // play_release is a core transport command, not a Player
                        // command: ask core to play this release at this track.
                        // The resulting Playback*/Queue events project into the
                        // player; no local player mutation here.
                        release?.let {
                            session.appHandle.playRelease(it.id, index.toUInt(), false)
                        }
                    },
                    onPlayRelease = {
                        release?.let { session.appHandle.playRelease(it.id, null, false) }
                    },
                    onShuffleRelease = {
                        release?.let { session.appHandle.playRelease(it.id, null, true) }
                    },
                    // Release-level queueing: hand core the release id so it
                    // expands the tracks (don't map them client-side).
                    onPlayReleaseNext = {
                        release?.let {
                            try {
                                session.appHandle.addReleaseNext(it.id)
                            } catch (e: Exception) {
                                Log.e(TAG, "addReleaseNext ${it.id} failed", e)
                            }
                        }
                    },
                    onAddReleaseToQueue = {
                        release?.let {
                            try {
                                session.appHandle.addReleaseToQueue(it.id)
                            } catch (e: Exception) {
                                Log.e(TAG, "addReleaseToQueue ${it.id} failed", e)
                            }
                        }
                    },
                    onPlayTrackNext = { trackId ->
                        try {
                            session.appHandle.addNext(listOf(trackId))
                        } catch (e: Exception) {
                            Log.e(TAG, "addNext $trackId failed", e)
                        }
                    },
                    onAddTrackToQueue = { trackId ->
                        try {
                            session.appHandle.addToQueue(listOf(trackId))
                        } catch (e: Exception) {
                            Log.e(TAG, "addToQueue $trackId failed", e)
                        }
                    },
                )
            }
        }

        NowPlayingBar(session = session)
    }
}

@Composable
private fun AlbumDetailContent(
    detail: BridgeAlbumDetail,
    selectedRelease: BridgeRelease?,
    coverPath: String?,
    currentTrackId: String?,
    isPlaying: Boolean,
    onSelectRelease: (String) -> Unit,
    onTogglePlayPause: () -> Unit,
    onPlayTrackAt: (Int) -> Unit,
    onPlayRelease: () -> Unit,
    onShuffleRelease: () -> Unit,
    onPlayReleaseNext: () -> Unit,
    onAddReleaseToQueue: () -> Unit,
    onPlayTrackNext: (String) -> Unit,
    onAddTrackToQueue: (String) -> Unit,
) {
    val album = detail.album
    val isCompilation = album.isCompilation

    var showGallery by remember { mutableStateOf(false) }
    val galleryItems = selectedRelease?.galleryItems ?: emptyList()
    if (showGallery && galleryItems.isNotEmpty()) {
        GalleryDialog(items = galleryItems, onDismiss = { showGallery = false })
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        item {
            Row(verticalAlignment = Alignment.Top) {
                Box(
                    modifier = Modifier
                        .size(140.dp)
                        .clip(RoundedCornerShape(6.dp))
                        .clickable(enabled = galleryItems.isNotEmpty()) { showGallery = true },
                    contentAlignment = Alignment.Center,
                ) {
                    if (coverPath != null) {
                        AsyncImage(
                            model = coverModel(coverPath),
                            contentDescription = album.title,
                            contentScale = ContentScale.Crop,
                            modifier = Modifier.fillMaxSize(),
                        )
                    } else {
                        Surface(
                            color = MaterialTheme.colorScheme.surfaceVariant,
                            modifier = Modifier.fillMaxSize(),
                        ) {
                            Icon(
                                imageVector = Icons.Filled.MusicNote,
                                contentDescription = null,
                                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                                modifier = Modifier.padding(32.dp),
                            )
                        }
                    }
                }
                Spacer(modifier = Modifier.width(16.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = album.title,
                        style = MaterialTheme.typography.headlineSmall,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        text = album.artistNames,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    val year = album.year
                    if (year != null) {
                        Text(
                            text = year.toString(),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    if (selectedRelease != null && selectedRelease.compactMetadata.isNotEmpty()) {
                        Spacer(modifier = Modifier.height(4.dp))
                        Text(
                            text = selectedRelease.compactMetadata,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }

        if (detail.releases.size > 1) {
            item {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    detail.releases.forEach { rel ->
                        FilterChip(
                            selected = rel.id == selectedRelease?.id,
                            onClick = { onSelectRelease(rel.id) },
                            label = { Text(rel.displayName, maxLines = 1) },
                        )
                    }
                }
            }
        }

        if (selectedRelease != null) {
            item {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(onClick = onPlayRelease) {
                        Icon(
                            imageVector = Icons.Filled.PlayArrow,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text("Play")
                    }
                    OutlinedButton(onClick = onShuffleRelease) {
                        Icon(
                            imageVector = Icons.Filled.Shuffle,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text("Shuffle")
                    }
                }
            }
            item {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = onPlayReleaseNext) {
                        Icon(
                            imageVector = Icons.AutoMirrored.Filled.PlaylistPlay,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text("Play Next")
                    }
                    OutlinedButton(onClick = onAddReleaseToQueue) {
                        Icon(
                            imageVector = Icons.Filled.Add,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text("Add to Queue")
                    }
                }
            }

            // Flatten groups to a release-wide track index so taps map to the
            // ordered list the player builds from the same flattening.
            var runningIndex = 0
            selectedRelease.trackGroups.forEach { group ->
                if (group.sideLabel.isNotEmpty()) {
                    item {
                        Text(
                            text = group.sideLabel,
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(top = 8.dp),
                        )
                    }
                }
                val groupOffset = runningIndex
                itemsIndexed(group.tracks, key = { _, t -> t.id }) { localIndex, track ->
                    val isCurrent = track.id == currentTrackId
                    TrackRow(
                        positionLabel = track.positionLabel,
                        title = track.title,
                        artistNames = if (isCompilation) track.artistNames else null,
                        durationLabel = track.durationLabel,
                        isCurrent = isCurrent,
                        isPlaying = isPlaying,
                        // Tapping the current track toggles play/pause; any other
                        // track plays the release from there.
                        onClick = {
                            if (isCurrent) {
                                onTogglePlayPause()
                            } else {
                                onPlayTrackAt(groupOffset + localIndex)
                            }
                        },
                        onPlayNext = { onPlayTrackNext(track.id) },
                        onAddToQueue = { onAddTrackToQueue(track.id) },
                    )
                }
                runningIndex += group.tracks.size
            }

            if (selectedRelease.totalDurationLabel.isNotEmpty()) {
                item {
                    Text(
                        text = selectedRelease.totalDurationLabel,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(top = 8.dp),
                    )
                }
            }
        }
    }
}

@Composable
private fun TrackRow(
    positionLabel: String,
    title: String,
    artistNames: String?,
    durationLabel: String,
    isCurrent: Boolean,
    isPlaying: Boolean,
    onClick: () -> Unit,
    onPlayNext: () -> Unit,
    onAddToQueue: () -> Unit,
) {
    var menuExpanded by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (isCurrent) {
            Box(modifier = Modifier.width(40.dp), contentAlignment = Alignment.CenterStart) {
                Icon(
                    imageVector = if (isPlaying) {
                        Icons.AutoMirrored.Filled.VolumeUp
                    } else {
                        Icons.Filled.PlayArrow
                    },
                    contentDescription = if (isPlaying) "Now playing" else "Paused",
                    tint = MaterialTheme.colorScheme.primary,
                )
            }
        } else {
            Text(
                text = positionLabel.ifEmpty { "-" },
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.width(40.dp),
            )
        }
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = title,
                style = MaterialTheme.typography.bodyMedium,
                color = if (isCurrent) {
                    MaterialTheme.colorScheme.primary
                } else {
                    MaterialTheme.colorScheme.onSurface
                },
                maxLines = 1,
            )
            if (artistNames != null) {
                Text(
                    text = artistNames,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                )
            }
        }
        if (durationLabel.isNotEmpty()) {
            Text(
                text = durationLabel,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Box {
            IconButton(onClick = { menuExpanded = true }) {
                Icon(
                    imageVector = Icons.Filled.MoreVert,
                    contentDescription = "Track options",
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            DropdownMenu(
                expanded = menuExpanded,
                onDismissRequest = { menuExpanded = false },
            ) {
                DropdownMenuItem(
                    text = { Text("Play Next") },
                    onClick = {
                        menuExpanded = false
                        onPlayNext()
                    },
                )
                DropdownMenuItem(
                    text = { Text("Add to Queue") },
                    onClick = {
                        menuExpanded = false
                        onAddToQueue()
                    },
                )
            }
        }
    }
}
