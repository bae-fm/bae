package fm.bae.app.ui.library

import androidx.annotation.StringRes
import fm.bae.app.R
import uniffi.bae_bridge.BridgeSharing

/** Who sees a playlist, in the words the listening history setting uses. */
@StringRes
internal fun BridgeSharing.labelRes(): Int =
    when (this) {
        BridgeSharing.PRIVATE -> R.string.playlist_sharing_private
        BridgeSharing.HOUSEHOLD -> R.string.playlist_sharing_household
    }

/** What the choice means, under the control that picks it. */
@StringRes
internal fun BridgeSharing.noteRes(): Int =
    when (this) {
        BridgeSharing.PRIVATE -> R.string.playlist_sharing_private_note
        BridgeSharing.HOUSEHOLD -> R.string.playlist_sharing_household_note
    }
