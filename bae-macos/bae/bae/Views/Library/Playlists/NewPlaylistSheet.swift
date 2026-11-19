import BaeKit
import SwiftUI

/// The New playlist sheet's content while it is open: the name typed, who
/// sees the playlist, the picked tracks it starts with, in no order, and the
/// error from a failed create. A new playlist starts on Only me.
struct NewPlaylistSheetState: Identifiable {
    let id = UUID()
    var name = ""
    var sharing: BridgeSharing = .private
    let pickedTrackIds: [String]
    var error: DisplayError?
}

/// Modal for making a playlist; the caller makes it and writes any error
/// back into `state`.
struct NewPlaylistSheet: View {
    @Binding
    var state: NewPlaylistSheetState
    let onCancel: () -> Void
    let onCreate: (NewPlaylistSheetState) -> Void

    var body: some View {
        MacSheet("New playlist", width: 420) {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                PlaylistFields(name: $state.name, sharing: $state.sharing)
                if let error = state.error {
                    MacNotice(error: error)
                }
            }
        } footer: {
            MacSheetFooter {
                Button("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Create") { onCreate(state) }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(enteredNameIsBlank(text: state.name))
            }
        }
    }
}
