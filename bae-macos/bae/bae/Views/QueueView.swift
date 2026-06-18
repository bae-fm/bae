import SwiftUI
import UniformTypeIdentifiers

struct QueueView: View {
    let isActive: Bool
    let nowPlayingTitle: String?
    let nowPlayingArtist: String?
    let nowPlayingPath: String?
    let items: [QueueItem]
    let resolveImagePath: (String?) -> String?
    let onClose: () -> Void
    let onClear: () -> Void
    let onSkipTo: (Int) -> Void
    let onRemove: (Int) -> Void
    let onReorder: (Int, Int) -> Void
    let onInsertTracks: ([String], Int) -> Void

    @State
    private var hoveredIndex: Int?
    @State
    private var draggedTrackId: String?
    @State
    private var dropInsertIndex: Int?

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()

            if isActive {
                nowPlayingSection
                Divider()
            }

            if items.isEmpty {
                ContentUnavailableView(
                    "Queue is empty",
                    systemImage: "list.bullet",
                    description: Text("Drag tracks here or play an album"),
                )
                .frame(maxHeight: .infinity)
                .dropDestination(for: String.self) { droppedIds, _ in
                    guard !droppedIds.isEmpty else {
                        return false
                    }
                    onInsertTracks(droppedIds, 0)
                    return true
                }
            }
            else {
                ScrollView {
                    VStack(spacing: 0) {
                        ForEach(Array(items.enumerated()), id: \.element.id) {
                            index,
                            item in
                            VStack(spacing: 0) {
                                // Insertion line above this row
                                if dropInsertIndex == index {
                                    insertionLine
                                }
                                queueItemRow(item, index: index)
                                    .padding(.horizontal, 12)
                                    .padding(.vertical, 4)
                                    .opacity(
                                        draggedTrackId == item.id ? 0.3 : 1.0
                                    )
                                Divider().padding(.leading, 62)
                            }
                            .onDrop(
                                of: [UTType.plainText],
                                delegate: QueueDropDelegate(
                                    targetIndex: index,
                                    items: items,
                                    draggedTrackId: $draggedTrackId,
                                    dropInsertIndex: $dropInsertIndex,
                                    onReorder: onReorder,
                                    onInsertTracks: onInsertTracks,
                                )
                            )
                        }
                        // Insertion line at the very end
                        if dropInsertIndex == items.count {
                            insertionLine
                        }

                        // Drop zone at the end of the list for appending
                        Color.clear
                            .frame(height: 40)
                            .onDrop(
                                of: [UTType.plainText],
                                delegate: QueueDropDelegate(
                                    targetIndex: items.count,
                                    items: items,
                                    draggedTrackId: $draggedTrackId,
                                    dropInsertIndex: $dropInsertIndex,
                                    onReorder: onReorder,
                                    onInsertTracks: onInsertTracks,
                                )
                            )
                    }
                }
                .background(Theme.background)
            }
        }
        .background(Theme.surface)
    }

    private var insertionLine: some View {
        Rectangle()
            .fill(Color.accentColor)
            .frame(height: 2)
            .padding(.horizontal, 8)
    }

    // MARK: - Header

    private var header: some View {
        HStack {
            Text("Queue")
                .font(.headline)
            Spacer()
            Button("Clear") { onClear() }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .disabled(items.isEmpty)
            Button(action: onClose) {
                Image(systemName: "xmark")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
        }
        .padding()
    }

    // MARK: - Now Playing

    private var nowPlayingSection: some View {
        HStack(spacing: 12) {
            nowPlayingArt
                .frame(width: 48, height: 48)
                .clipShape(RoundedRectangle(cornerRadius: 4))

            VStack(alignment: .leading, spacing: 2) {
                Text("Now Playing")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                if let title = nowPlayingTitle {
                    Text(title)
                        .font(.callout)
                        .fontWeight(.medium)
                        .lineLimit(1)
                }
                if let artist = nowPlayingArtist {
                    Text(artist)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }

            Spacer()
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }

    private var nowPlayingArt: some View {
        ImageView(localPath: nowPlayingPath, pointSize: 48)
    }

    // MARK: - Queue Items

    private func queueItemRow(_ item: QueueItem, index: Int) -> some View {
        HStack(spacing: 10) {
            ZStack {
                queueItemArt(path: resolveImagePath(item.coverImageId))
                    .frame(width: 40, height: 40)
                    .clipShape(RoundedRectangle(cornerRadius: 3))

                if hoveredIndex == index {
                    RoundedRectangle(cornerRadius: 3)
                        .fill(.black.opacity(0.5))
                        .frame(width: 40, height: 40)
                    Button(action: { onSkipTo(index) }) {
                        Image(systemName: "play.fill")
                            .font(.caption)
                            .foregroundColor(.white)
                    }
                    .buttonStyle(.plain)
                }
            }

            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .font(.callout)
                    .lineLimit(1)
                Text(item.albumTitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Spacer()

            if hoveredIndex == index {
                Button(action: { onRemove(index) }) {
                    Image(systemName: "xmark")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .help("Remove from queue")
            }
            else if !item.durationLabel.isEmpty {
                Text(item.durationLabel)
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
        }
        .contentShape(Rectangle())
        .onHover { isHovered in
            hoveredIndex = isHovered ? index : nil
        }
        .onDrag {
            draggedTrackId = item.id
            return NSItemProvider(object: item.id as NSString)
        }
        .onTapGesture(count: 2) {
            onSkipTo(index)
        }
        .contextMenu {
            Button("Remove from Queue") {
                onRemove(index)
            }
        }
    }

    private func queueItemArt(path: String?) -> some View {
        ImageView(localPath: path, pointSize: 40)
    }
}

// MARK: - Drop Delegate

private struct QueueDropDelegate: DropDelegate {
    let targetIndex: Int
    let items: [QueueItem]
    @Binding
    var draggedTrackId: String?
    @Binding
    var dropInsertIndex: Int?
    let onReorder: (Int, Int) -> Void
    let onInsertTracks: ([String], Int) -> Void

    /// Whether this is an internal reorder (dragged from within the queue).
    private var isInternalDrag: Bool {
        guard let draggedId = draggedTrackId else {
            return false
        }
        return items.contains { $0.id == draggedId }
    }

    func dropEntered(info _: DropInfo) {
        if let draggedId = draggedTrackId,
            let fromIndex = items.firstIndex(where: { $0.id == draggedId })
        {
            // Internal reorder: show insertion line relative to source
            if targetIndex > fromIndex {
                dropInsertIndex = targetIndex + 1
            }
            else if targetIndex < fromIndex {
                dropInsertIndex = targetIndex
            }
            else {
                dropInsertIndex = nil
            }
        }
        else {
            // External drop: show insertion line at target position
            dropInsertIndex = targetIndex
        }
    }

    func dropUpdated(info _: DropInfo) -> DropProposal? {
        DropProposal(operation: isInternalDrag ? .move : .copy)
    }

    func performDrop(info: DropInfo) -> Bool {
        if let draggedId = draggedTrackId,
            let fromIndex = items.firstIndex(where: { $0.id == draggedId })
        {
            // Internal reorder
            let toIndex =
                targetIndex > fromIndex ? targetIndex + 1 : targetIndex
            if toIndex != fromIndex {
                onReorder(fromIndex, toIndex)
            }
            draggedTrackId = nil
            dropInsertIndex = nil
            return true
        }

        // External drop: load string IDs from the pasteboard
        let providers = info.itemProviders(for: [UTType.plainText])
        guard !providers.isEmpty else {
            dropInsertIndex = nil
            return false
        }

        let insertAt = targetIndex
        let serialQueue = DispatchQueue(label: "bae.queue-drop-collect")
        nonisolated(unsafe) var collectedIds: [String] = []
        let group = DispatchGroup()

        for provider in providers {
            group.enter()
            provider.loadItem(forTypeIdentifier: UTType.plainText.identifier) {
                item,
                _ in
                if let data = item as? Data,
                    let str = String(data: data, encoding: .utf8)
                {
                    serialQueue.sync { collectedIds.append(str) }
                }
                else if let str = item as? String {
                    serialQueue.sync { collectedIds.append(str) }
                }
                group.leave()
            }
        }

        group.notify(queue: .main) {
            if !collectedIds.isEmpty {
                onInsertTracks(collectedIds, insertAt)
            }
        }

        dropInsertIndex = nil
        return true
    }

    func dropExited(info _: DropInfo) {
        dropInsertIndex = nil
    }

    func validateDrop(info: DropInfo) -> Bool {
        // Accept both internal drags and external drops with plain text
        if draggedTrackId != nil {
            return true
        }
        return info.hasItemsConforming(to: [UTType.plainText])
    }
}

#if DEBUG
    extension QueueView {
        /// Preview builder — fixes the image resolver and the action callbacks
        /// to inert defaults so a preview states only its queue state.
        static func preview(
            isActive: Bool,
            nowPlayingTitle: String?,
            nowPlayingArtist: String?,
            items: [QueueItem]
        ) -> QueueView {
            QueueView(
                isActive: isActive,
                nowPlayingTitle: nowPlayingTitle,
                nowPlayingArtist: nowPlayingArtist,
                nowPlayingPath: nil,
                items: items,
                resolveImagePath: { _ in nil },
                onClose: {},
                onClear: {},
                onSkipTo: { _ in },
                onRemove: { _ in },
                onReorder: { _, _ in },
                onInsertTracks: { _, _ in }
            )
        }
    }
#endif

// MARK: - Previews

#Preview("With items") {
    QueueView.preview(
        isActive: true,
        nowPlayingTitle: PreviewData.nowPlayingTitle,
        nowPlayingArtist: PreviewData.nowPlayingArtist,
        items: PreviewData.queueItems
    )
    .frame(width: 350, height: 500)
    .environment(MediaPaths.stub)
}

#Preview("Empty") {
    QueueView.preview(
        isActive: false,
        nowPlayingTitle: nil,
        nowPlayingArtist: nil,
        items: []
    )
    .frame(width: 350, height: 400)
    .environment(MediaPaths.stub)
}
