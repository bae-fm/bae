import SwiftUI

/// One line per folder chosen to import that is still being read. Choosing a
/// folder waits for its read before going anywhere, so without this a large
/// folder would look like nothing happened. The window stays usable meanwhile.
struct FolderReadingBanner: View {
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        if !uiStore.foldersBeingRead.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(uiStore.foldersBeingRead) { folder in
                    HStack(spacing: 8) {
                        ProgressView()
                            .controlSize(.small)
                        Text("Reading “\(folder.name)”…")
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Spacer()
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .background(.bar)
        }
    }
}

#if DEBUG
    #Preview("Folder reading banner") {
        let uiStore = UiStore()
        _ = uiStore.beginReadingFolder(named: "Album")
        return FolderReadingBanner()
            .environment(uiStore)
            .frame(width: 600)
    }
#endif
