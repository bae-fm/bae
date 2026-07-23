import BaeKit
import SwiftUI

/// A collapsible section header for one watched folder in the candidate list:
/// disclosure chevron, folder name, and count, with reveal/remove in its
/// context menu.
struct FolderSectionHeader: View {
    let folder: BridgeWatchedFolder
    let count: Int
    let isCollapsed: Bool
    let onToggle: () -> Void
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: 5) {
            Image(systemName: "chevron.right")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(.tertiary)
                .rotationEffect(.degrees(isCollapsed ? 0 : 90))
            Image(systemName: "folder")
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(folder.name)
                .font(.caption)
                .fontWeight(.semibold)
                .textCase(.uppercase)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer()
            Text(verbatim: count.formatted())
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.tertiary)
        }
        .contentShape(Rectangle())
        .onTapGesture(perform: onToggle)
        .help(folder.path)
        .contextMenu {
            Button("Reveal in Finder") {
                SystemActions.revealInFinder(path: folder.path)
            }
            Divider()
            Button("Remove Folder", role: .destructive, action: onRemove)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Expanded") {
        FolderSectionHeader(
            folder: PreviewData.importWatchedFolder,
            count: 12,
            isCollapsed: false,
            onToggle: {},
            onRemove: {},
        )
        .padding()
        .frame(width: 320)
        .windowBackground()
    }

    #Preview("Collapsed") {
        FolderSectionHeader(
            folder: PreviewData.importWatchedFolder,
            count: 12,
            isCollapsed: true,
            onToggle: {},
            onRemove: {},
        )
        .padding()
        .frame(width: 320)
        .windowBackground()
    }
#endif
