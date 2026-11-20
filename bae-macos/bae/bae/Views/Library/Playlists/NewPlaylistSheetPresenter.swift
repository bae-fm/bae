import BaeKit
import SwiftUI

extension View {
    /// Show the New playlist sheet over this view while `UiStore` holds its
    /// state, and open the playlist it makes in the side pane.
    func newPlaylistSheet() -> some View {
        modifier(NewPlaylistSheetPresenter())
    }
}

/// The New playlist sheet over the window, from wherever the person asked
/// for it: the side pane's +, or a track's Add to playlist menu.
private struct NewPlaylistSheetPresenter: ViewModifier {
    @Environment(Library.self)
    private var library
    @Environment(UiStore.self)
    private var uiStore

    func body(content: Content) -> some View {
        content.sheet(
            item: Binding(
                get: { uiStore.newPlaylistSheet },
                set: { uiStore.newPlaylistSheet = $0 }
            )
        ) { sheet in
            NewPlaylistSheet(
                // While the sheet closes, it shows the state it had before
                // `newPlaylistSheet` became nil.
                state: Binding(
                    get: { uiStore.newPlaylistSheet ?? sheet },
                    set: { uiStore.newPlaylistSheet = $0 }
                ),
                onCancel: { uiStore.newPlaylistSheet = nil },
                onCreate: create
            )
        }
    }

    /// Make the playlist `sheet` describes and open its page in the side
    /// pane; a failure stays in the sheet, which stays open.
    private func create(_ sheet: NewPlaylistSheetState) {
        let library = library
        let uiStore = uiStore
        Task {
            do {
                let playlistId = try await library.playlistWrites
                    .createOfPicked(
                        sheet.name,
                        sheet.sharing,
                        sheet.pickedTrackIds
                    )
                uiStore.openPlaylist(playlistId)
                uiStore.newPlaylistSheet = nil
            }
            catch {
                uiStore.newPlaylistSheet?.error = DisplayError(error)
            }
        }
    }
}
