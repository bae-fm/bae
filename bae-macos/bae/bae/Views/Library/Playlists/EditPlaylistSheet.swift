import BaeKit
import SwiftUI

/// The Edit playlist sheet's content while it is open: the playlist, the
/// name and who sees it as typed so far, and the error from a failed save.
struct EditPlaylistSheetState: Identifiable {
    let id = UUID()
    let playlistId: String
    var name: String
    var sharing: BridgeSharing
    var error: DisplayError?

    init(_ playlist: BridgePlaylistSummary) {
        playlistId = playlist.id
        name = playlist.name
        sharing = playlist.sharing
    }
}

/// A sheet that changes a playlist's name and who sees it. The caller saves
/// the changes and writes any error back into `state`.
struct EditPlaylistSheet: View {
    @Binding
    var state: EditPlaylistSheetState
    let onCancel: () -> Void
    let onSave: (EditPlaylistSheetState) -> Void

    var body: some View {
        MacSheet("Edit playlist", width: 420) {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                PlaylistFields(name: $state.name, sharing: $state.sharing)
                if let error = state.error {
                    MacNotice(error: error)
                }
            }
        } footer: {
            MacSheetFooter {
                MacButton("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                MacButton("Save", kind: .primary) { onSave(state) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(enteredNameIsBlank(text: state.name))
            }
        }
    }
}
