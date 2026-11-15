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

/// Modal for changing a playlist's name and who sees it; the caller saves it
/// and writes any error back into `state`.
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
                    Notice(error: error)
                }
            }
        } footer: {
            MacSheetFooter {
                Button("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Save") { onSave(state) }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(enteredNameIsBlank(text: state.name))
            }
        }
    }
}
