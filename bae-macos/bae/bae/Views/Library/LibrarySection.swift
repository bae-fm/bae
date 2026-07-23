import BaeKit
import SwiftUI

struct LibrarySection: View {
    var body: some View {
        LibraryView()
            .toolbar(.hidden)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

#if DEBUG
    #Preview("Library Section") {
        let uiStore = UiStore()
        let libraryStore = LibraryStore()
        let backing = LibraryView.previewGridBacking(
            uiStore: uiStore,
            libraryStore: libraryStore
        )
        // Bind the tuple members to explicitly-typed locals: the audit resolves
        // `.environment(library)`/`.environment(session)` by the local's type
        // but leaves a bare `backing.library` member access unresolved.
        let library: Library = backing.library
        let session: LibraryBrowseSession = backing.session
        return LibrarySection()
            .environment(MediaPaths.stub)
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(library)
            .environment(libraryStore)
            .environment(uiStore)
            .environment(session)
            .environment(PreviewData.configStore)
            .frame(width: 1400, height: 720)
            .windowBackground()
    }
#endif
