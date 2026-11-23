package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.data.LiveQueryState
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidContentUnavailable
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.retryAction
import uniffi.bae_bridge.BridgeAlbumDetail

/** What the album page shows while it has no album to draw. */
internal sealed interface AlbumDetailPlaceholder {
    data object Loading : AlbumDetailPlaceholder

    /** The read failed; Retry reads it again. */
    data class Failed(
        val error: DisplayError,
    ) : AlbumDetailPlaceholder

    /** The read answered that no such album exists: it was deleted, here or on another device. */
    data object NotInLibrary : AlbumDetailPlaceholder
}

/** The placeholder for [query] while it holds no album, given its failure as [loadError]. */
internal fun albumDetailPlaceholder(
    query: LiveQueryState<BridgeAlbumDetail?>,
    loadError: DisplayError?,
): AlbumDetailPlaceholder =
    when {
        loadError != null -> AlbumDetailPlaceholder.Failed(loadError)
        query is LiveQueryState.Answered && query.value == null -> AlbumDetailPlaceholder.NotInLibrary
        else -> AlbumDetailPlaceholder.Loading
    }

/** A deleted album says so, with the top bar's back as the way out, like a missing artist or composer. */
@Composable
internal fun BoxScope.AlbumDetailPlaceholderBox(
    placeholder: AlbumDetailPlaceholder,
    onRetry: () -> Unit,
) {
    when (placeholder) {
        is AlbumDetailPlaceholder.Failed -> {
            AndroidNotice(
                error = placeholder.error,
                look = NoticeLook.CENTERED,
                action = placeholder.error.retryAction(onClick = onRetry),
            )
        }

        AlbumDetailPlaceholder.NotInLibrary -> {
            AndroidContentUnavailable(
                message = stringResource(R.string.album_not_in_library),
                modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
            )
        }

        // The album is a local read; nothing shows until it lands.
        AlbumDetailPlaceholder.Loading -> {}
    }
}
