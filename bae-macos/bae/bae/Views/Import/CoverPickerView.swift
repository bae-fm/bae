import BaeKit
import SwiftUI

/// A gallery-style picker for selecting cover art from remote and local sources.
/// Presented as a sheet from the import confirmation view.
struct CoverPickerView: View {
    let remoteCoverArts: [BridgeRemoteCover]
    let localArtwork: [BridgeArtworkFile]
    let selectedCover: BridgeCoverChoice?
    let onSelect: (BridgeCoverChoice) -> Void
    let onDone: () -> Void

    @State
    private var cursor: Cursor<CoverItem>?

    private var items: [CoverItem] {
        var result: [CoverItem] = []
        for cover in remoteCoverArts {
            result.append(
                CoverItem(
                    coverChoice: cover.coverChoice,
                    label: cover.label
                )
            )
        }
        for file in localArtwork {
            // Local artwork is collected from the candidate's scanned folder,
            // so it's always present on disk.
            result.append(
                CoverItem(
                    coverChoice: file.coverChoice,
                    label: file.file.name
                )
            )
        }
        return result
    }

    private var selectedItemId: BridgeCoverSelection? {
        selectedCover?.selection
    }

    private func rebuild() {
        cursor = Cursor(
            items: items,
            preferring: cursor?.current.id ?? selectedItemId
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Spacer()
                Button("Done") { onDone() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)

            Divider()

            if let cursor {
                // Large preview
                GeometryReader { geometry in
                    let previewHeight = geometry.size.height - 120
                    ZStack {
                        coverPreview(for: cursor.current)
                            .frame(
                                maxWidth: geometry.size.width - 40,
                                maxHeight: previewHeight
                            )
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }

                // Source label + count
                VStack(spacing: 8) {
                    Text(
                        "\(cursor.current.label) \u{2014} \(cursor.index + 1) of \(cursor.items.count)"
                    )
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)

                    // Thumbnail strip
                    if cursor.canCycle {
                        ThumbnailStrip(
                            cursor: cursor,
                            centered: false,
                            onSelect: { self.cursor?.select(id: $0) },
                            stroke: { item, isActive in
                                item.id == selectedItemId
                                    ? (Color.accentColor, 2)
                                    : isActive
                                        ? (Color.primary, 2) : (Color.clear, 0)
                            }
                        ) { item in
                            ImageView(
                                source: item.thumbnailSource,
                                pointSize: 56
                            )
                        }
                    }

                    // Use This Cover button
                    HStack {
                        Spacer()
                        Button("Use This Cover") {
                            onSelect(cursor.current.coverChoice)
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(cursor.current.id == selectedItemId)
                    }
                    .padding(.horizontal, 16)
                }
                .padding(.bottom, 12)
            }
            else {
                ContentUnavailableView(
                    "No cover art available",
                    systemImage: "photo",
                    description: Text("No remote or local artwork found"),
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Theme.background)
        .onAppear { rebuild() }
        .onChange(of: items) { _, _ in rebuild() }
        .onKeyPress(.leftArrow) {
            cursor?.goToPrevious()
            return .handled
        }
        .onKeyPress(.rightArrow) {
            cursor?.goToNext()
            return .handled
        }
        .onKeyPress(.escape) {
            onDone()
            return .handled
        }
    }

    @ViewBuilder
    private func coverPreview(for item: CoverItem) -> some View {
        ImageView(source: item.previewSource, contentMode: .fit, pointSize: 600)
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .shadow(radius: 10)
    }

}

#if DEBUG
    // MARK: - Previews

    #Preview("With covers") {
        CoverPickerView(
            remoteCoverArts: PreviewData.remoteCovers,
            localArtwork: PreviewData.bridgeCandidateFiles.artwork,
            selectedCover: PreviewData.remoteCovers.first?.coverChoice,
            onSelect: { _ in },
            onDone: {},
        )
        .frame(width: 520, height: 620)
        .importPreviewEnvironment()
    }

    #Preview("Empty") {
        CoverPickerView(
            remoteCoverArts: [],
            localArtwork: [],
            selectedCover: nil,
            onSelect: { _ in },
            onDone: {},
        )
        .frame(width: 520, height: 620)
        .importPreviewEnvironment()
    }
#endif
