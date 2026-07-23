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
        return LibrarySection()
            .environment(MediaPaths.stub)
            .environment(Playback.stub)
            .environment(Queue.stub)
            .environment(Downloads.stub)
            .environment(backing.library)
            .environment(libraryStore)
            .environment(uiStore)
            .environment(backing.session)
            .environment(PreviewData.configStore)
            .frame(width: 1400, height: 720)
            .windowBackground()
    }
#endif
