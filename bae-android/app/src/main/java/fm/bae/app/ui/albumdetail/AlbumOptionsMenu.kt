package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidDivider
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.AndroidMenu
import fm.bae.app.ui.components.AndroidMenuAction
import fm.bae.app.ui.components.AndroidMenuHeading
import fm.bae.app.ui.components.DialogAction
import fm.bae.app.ui.components.WIDE_DIALOG_WIDTH

/** The menu's Pin or Unpin item for the release in view. */
internal class PinMenuItem(
    val pinned: Boolean,
    val onClick: () -> Unit,
)

/** The menu's first group: what plays the release or queues it. */
internal class PlayMenuActions(
    val onShuffle: () -> Unit,
    val onPlayNext: () -> Unit,
    val onAddToQueue: () -> Unit,
)

internal sealed interface AlbumRatingAction {
    class Rate(val onClick: () -> Unit) : AlbumRatingAction

    class Heart(
        val hearters: String,
        val hearted: Boolean,
        val onChoose: (Boolean) -> Unit,
    ) : AlbumRatingAction
}

/**
 * What the album's menu does. [play] is absent when the release cannot play on this device.
 * [pin] is `null` while the page's own control is the way to act on the
 * release's pin, and [onSetPrimaryRelease] is `null` when the release in view already is the album's
 * primary one or the album holds one release.
 */
internal class AlbumMenuActions(
    val play: PlayMenuActions?,
    val rating: AlbumRatingAction?,
    val pin: PinMenuItem?,
    val onSetPrimaryRelease: (() -> Unit)?,
    val onDelete: () -> Unit,
)

/**
 * The album's options, the ⋮ at the end of the page's bar: Shuffle, Play next and Add to queue, then
 * Rate, Pin or Unpin and Set as primary release, then Delete album….
 */
@Composable
internal fun AlbumOptionsMenu(actions: AlbumMenuActions) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        AndroidIconButton(SymbolIcons.MoreVert, stringResource(R.string.album_options), onClick = { expanded = true })
        AndroidMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            AlbumMenuItems(actions, close = { expanded = false })
        }
    }
}

/** The menu contents, also drawn by the shots that show an open menu. */
@Composable
internal fun AlbumMenuItems(
    actions: AlbumMenuActions,
    close: () -> Unit,
) {
    actions.play?.let { play ->
        AndroidMenuAction(stringResource(R.string.shuffle), close, play.onShuffle)
        AndroidMenuAction(stringResource(R.string.play_next), close, play.onPlayNext)
        AndroidMenuAction(stringResource(R.string.add_to_queue), close, play.onAddToQueue)
    }
    val hasMiddleGroup = actions.rating != null || actions.pin != null || actions.onSetPrimaryRelease != null
    if (actions.play != null && hasMiddleGroup) AndroidDivider()
    when (val rating = actions.rating) {
        null -> Unit
        is AlbumRatingAction.Rate -> AndroidMenuAction(stringResource(R.string.rate), close, rating.onClick)
        is AlbumRatingAction.Heart -> {
            if (rating.hearters.isNotEmpty()) AndroidMenuHeading(rating.hearters)
            val label = if (rating.hearted) R.string.album_remove_heart else R.string.album_add_heart
            AndroidMenuAction(stringResource(label), close) { rating.onChoose(!rating.hearted) }
        }
    }
    actions.pin?.let {
        AndroidMenuAction(stringResource(if (it.pinned) R.string.unpin else R.string.pin), close, it.onClick)
    }
    actions.onSetPrimaryRelease?.let {
        AndroidMenuAction(stringResource(R.string.set_primary_release), close, it)
    }
    if (actions.play != null || hasMiddleGroup) AndroidDivider()
    AndroidMenuAction(stringResource(R.string.delete_album), close, actions.onDelete)
}

/** Asks before an album is deleted: what goes with it, what stays, and that it can't be undone. */
@Composable
internal fun DeleteAlbumDialog(
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        width = WIDE_DIALOG_WIDTH,
        onDismissRequest = onDismiss,
        title = stringResource(R.string.delete_album_confirm_title),
        message = stringResource(R.string.delete_album_confirm_body),
        action = DialogAction(stringResource(R.string.delete_album_confirm), onConfirm, destructive = true),
        dismiss = DialogAction(stringResource(R.string.cancel), onDismiss),
    )
}
