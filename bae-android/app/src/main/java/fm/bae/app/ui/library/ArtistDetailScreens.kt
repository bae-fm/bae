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
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.CoverBytesCache
import fm.bae.app.ui.components.CoverImage
import fm.bae.app.ui.components.LocalCoverBytesCache
import fm.bae.app.ui.playback.NowPlayingBar
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeArtistDetail

private const val TAG = "bae.ArtistDetailScreens"
private val logger = BaeLogger(TAG)

@Composable
internal fun ArtistDetailScreen(
    session: OpenLibrary,
    artistId: String,
    onBack: () -> Unit,
    onSelectAlbum: (String) -> Unit,
) {
    var detail by remember(artistId) { mutableStateOf<BridgeArtistDetail?>(null) }
    var loadError by remember(artistId) { mutableStateOf<String?>(null) }
    val appContext = androidx.compose.ui.platform.LocalContext.current
    LaunchedEffect(artistId) {
        loadError = null
        try {
            detail = withContext(Dispatchers.IO) { session.library.artistDetail(artistId) }
            if (detail == null) {
                loadError = appContext.getString(R.string.artist_detail_not_found)
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed to load artist detail $artistId", e)
            loadError = appContext.getString(R.string.artist_detail_load_failed)
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
                ArtistDetailContent(
                    detail = loaded,
                    loadImage = session.library::imageBytes,
                    onSelectAlbum = onSelectAlbum,
                )
            }
        }
        NowPlayingBar(session = session)
    }
}

@Composable
private fun ArtistDetailContent(
    detail: BridgeArtistDetail,
    loadImage: suspend (imageId: String) -> ByteArray?,
    onSelectAlbum: (String) -> Unit,
) {
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        item {
            ArtistSummaryRow(
                artist = detail.artist,
                loadImage = loadImage,
                onClick = null,
            )
        }
        if (detail.albums.isNotEmpty()) {
            item { LibrarySectionHeader(stringResource(R.string.search_section_albums)) }
            items(detail.albums, key = { it.id }) { album ->
                Row(
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .clickable { onSelectAlbum(album.id) }
                            .padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CoverImage(
                        coverId = album.cover?.id,
                        coverVersion = album.cover?.version,
                        loadImage = loadImage,
                        cornerRadius = 6.dp,
                        iconPadding = 12.dp,
                        modifier = Modifier.size(48.dp),
                        contentDescription = album.title,
                    )
                    Spacer(modifier = Modifier.width(12.dp))
                    TwoLineText(title = album.title, subtitle = album.year?.toString())
                }
            }
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun ArtistDetailContentPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalCoverBytesCache provides CoverBytesCache()) {
            ArtistDetailContent(
                detail = PreviewData.artistDetail(),
                loadImage = { _ -> null },
                onSelectAlbum = {},
            )
        }
    }
}
