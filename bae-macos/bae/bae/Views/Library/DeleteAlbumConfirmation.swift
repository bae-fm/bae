import SwiftUI

/// The alert that asks before an album is deleted: what goes with it, what
/// stays, and that it can't be undone. It shows while `albumId` names an
/// album; Delete hands that album to `onDelete`, and either button clears
/// `albumId`.
struct DeleteAlbumConfirmation: ViewModifier {
    @Binding
    var albumId: String?
    let onDelete: (String) -> Void

    func body(content: Content) -> some View {
        content.alert(
            "Delete Album",
            isPresented: Binding(
                get: { albumId != nil },
                set: { presented in
                    if !presented { albumId = nil }
                }
            ),
            presenting: albumId
        ) { albumId in
            Button("Delete", role: .destructive) { onDelete(albumId) }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text(
                "Every release of this album is deleted from the library, along with the copies of their files that bae keeps on this Mac and in your storage. Files in your own folders aren't deleted. This cannot be undone."
            )
        }
    }
}

extension View {
    /// Ask before deleting the album `albumId` names; see
    /// `DeleteAlbumConfirmation`.
    func deleteAlbumConfirmation(
        albumId: Binding<String?>,
        onDelete: @escaping (String) -> Void
    ) -> some View {
        modifier(DeleteAlbumConfirmation(albumId: albumId, onDelete: onDelete))
    }
}

#if DEBUG
    private struct DeleteAlbumConfirmationPreview: View {
        @State
        private var albumId: String? = "album-id"

        var body: some View {
            Button {
                albumId = "album-id"
            } label: {
                Text(verbatim: "Ask Again")
            }
            .frame(width: 480, height: 320)
            .deleteAlbumConfirmation(albumId: $albumId) { _ in }
        }
    }

    #Preview("Delete Album Confirmation") {
        DeleteAlbumConfirmationPreview()
    }
#endif
