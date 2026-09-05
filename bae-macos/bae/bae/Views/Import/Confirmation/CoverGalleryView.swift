import BaeKit
import SwiftUI

/// Shared cover browser. Preview focus is separate from the saved selection.
struct CoverGalleryView: View {
    let remoteItems: RemoteCoverItems
    let releaseItems: [CoverItem]
    let selectedCover: BridgeCoverSelection?
    var isSaving = false
    var errorMessage: String?
    var onRefresh: (() -> Void)?
    var onFindRelease: (() -> Void)?
    let onSelect: (CoverItem) -> Void
    let onDone: () -> Void

    @State
    private var cursor: Cursor<CoverItem>?
    @State
    private var isLightboxPresented = false
    @FocusState
    private var previewFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Text("Change Cover").font(.title2.weight(.semibold))
                Spacer()
                if let onRefresh, remoteItems.canRefresh {
                    Button(action: onRefresh) {
                        Label("Refresh", systemImage: "arrow.clockwise")
                    }
                    .disabled(remoteItems.isLoading || isSaving)
                }
                Button("Done", action: onDone)
                    .keyboardShortcut(.cancelAction)
                    .disabled(isSaving)
            }
            .padding(24)
            Divider()
            HStack(spacing: 0) {
                ScrollView {
                    VStack(alignment: .leading, spacing: 28) {
                        section(
                            "Remote Sources",
                            icon: "globe",
                            items: remoteItems.items
                        )
                        remoteStatus
                        section(
                            "Release Files",
                            icon: "folder",
                            items: releaseItems
                        )
                    }
                    .padding(24)
                }
                .frame(maxWidth: .infinity)
                Divider()
                preview
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .padding(24)
                    .background(Theme.well)
            }
            Divider()
            HStack(spacing: 16) {
                if let errorMessage = errorMessage ?? remoteItems.failureMessage
                {
                    Label(errorMessage, systemImage: "exclamationmark.triangle")
                        .font(.callout)
                        .foregroundStyle(.red)
                        .textSelection(.enabled)
                }
                Spacer(minLength: 12)
                ProgressView().controlSize(.small).opacity(isSaving ? 1 : 0)
                Button("Use This Cover") {
                    if let cursor { onSelect(cursor.current) }
                }
                .buttonStyle(PrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
                .disabled(
                    cursor == nil || cursor?.current.id == selectedCover
                        || isSaving
                )
            }
            .padding(24)
        }
        .background(Theme.background)
        .disabled(isLightboxPresented)
        .accessibilityHidden(isLightboxPresented)
        .overlay {
            if isLightboxPresented, let cursor {
                LightboxView(
                    cursor: cursor,
                    onUpdate: { self.cursor = $0 },
                    onDismiss: {
                        isLightboxPresented = false
                        previewFocused = true
                    }
                )
            }
        }
        .onAppear { rebuild() }
        .onChange(of: remoteItems) { _, _ in rebuild() }
        .onChange(of: releaseItems) { _, _ in rebuild() }
        .onKeyPress(.leftArrow) {
            guard !isLightboxPresented else { return .ignored }
            cursor?.goToPrevious()
            return .handled
        }
        .onKeyPress(.rightArrow) {
            guard !isLightboxPresented else { return .ignored }
            cursor?.goToNext()
            return .handled
        }
    }

    private func rebuild() {
        cursor = Cursor(
            items: remoteItems.items + releaseItems,
            preferring: cursor?.current.id ?? selectedCover
        )
    }

    private func section(
        _ title: LocalizedStringKey,
        icon: String,
        items: [CoverItem]
    ) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Label(title, systemImage: icon)
                    .font(.headline)
                Spacer()
                Text(items.count, format: .number)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 130), spacing: 16)],
                spacing: 20
            ) {
                ForEach(items) { item in
                    tile(for: item)
                }
            }
        }
    }

    private func tile(for item: CoverItem) -> some View {
        Button {
            cursor?.select(id: item.id)
        } label: {
            VStack(alignment: .leading, spacing: 8) {
                ImageView(
                    content: item.thumbnailContent,
                    contentMode: .fit,
                    pointSize: 180
                )
                .frame(height: 138)
                .frame(maxWidth: .infinity)
                .padding(8)
                .background(Theme.hover)
                .clipShape(RoundedRectangle(cornerRadius: 8))
                .overlay {
                    RoundedRectangle(cornerRadius: 8)
                        .strokeBorder(
                            cursor?.current.id == item.id
                                ? Color.accentColor : .clear,
                            lineWidth: 2
                        )
                }
                .overlay(alignment: .topTrailing) {
                    Image(systemName: "checkmark.circle.fill")
                        .symbolRenderingMode(.palette)
                        .foregroundStyle(.white, Color.accentColor)
                        .padding(8)
                        .opacity(item.id == selectedCover ? 1 : 0)
                }
                Text(verbatim: item.label)
                    .font(.callout)
                    .lineLimit(2)
                    .truncationMode(.middle)
                    .frame(height: 34, alignment: .topLeading)
                Text(verbatim: item.sourceLabel)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(item.label)
        .accessibilityLabel(item.label)
        .accessibilityAddTraits(
            cursor?.current.id == item.id ? .isSelected : []
        )
        .disabled(isSaving)
        .simultaneousGesture(
            TapGesture(count: 2)
                .onEnded {
                    cursor?.select(id: item.id)
                    isLightboxPresented = true
                }
        )
    }

}

extension CoverGalleryView {
    @ViewBuilder
    private var remoteStatus: some View {
        switch remoteItems {
        case .loading:
            ProgressView("Fetching covers...").controlSize(.small)
        case .unlinked:
            VStack(alignment: .leading, spacing: 10) {
                Text("No linked release")
                    .font(.headline)
                Text(
                    "Link a Discogs or MusicBrainz release to browse its artwork."
                )
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                if let onFindRelease {
                    Button("Find release…", action: onFindRelease)
                        .disabled(isSaving)
                }
            }
        case .linked(let items) where items.isEmpty:
            Text("No remote covers found").foregroundStyle(.secondary)
        case .linked, .failed:
            EmptyView()
        }
    }

    @ViewBuilder
    private var preview: some View {
        if let cursor {
            VStack(spacing: 20) {
                Button {
                    isLightboxPresented = true
                } label: {
                    ImageView(
                        content: cursor.current.previewContent,
                        contentMode: .fit,
                        pointSize: 800
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .clipped()
                    .contentShape(Rectangle())
                    .overlay(alignment: .topTrailing) {
                        Image(systemName: "arrow.up.left.and.arrow.down.right")
                            .padding(10)
                            .background(.regularMaterial, in: Circle())
                            .padding(8)
                    }
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("View in Lightbox"))
                .help("View in Lightbox")
                .keyboardShortcut(.space, modifiers: [])
                .focused($previewFocused)
                .disabled(isSaving)
                Text(verbatim: cursor.current.label)
                    .font(.headline)
                    .multilineTextAlignment(.center)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                Text(verbatim: cursor.current.sourceLabel)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                HStack(spacing: 20) {
                    Button {
                        self.cursor?.goToPrevious()
                    } label: {
                        Image(systemName: "chevron.left")
                    }
                    .accessibilityLabel(Text("Previous image"))
                    Text("\(cursor.index + 1) of \(cursor.items.count)")
                        .monospacedDigit().foregroundStyle(.secondary)
                    Button {
                        self.cursor?.goToNext()
                    } label: {
                        Image(systemName: "chevron.right")
                    }
                    .accessibilityLabel(Text("Next image"))
                }
                .buttonStyle(.borderless)
                .disabled(!cursor.canCycle || isSaving)
            }
        }
        else {
            ContentUnavailableView(
                "No cover art available",
                systemImage: "photo"
            )
        }
    }
}

/// Bounds cover browsers to the available modal host, including short windows.
struct CoverPickerFrame<Content: View>: View {
    @ViewBuilder
    let content: () -> Content

    var body: some View {
        GeometryReader { geometry in
            content()
                .frame(
                    width: min(1_100, max(0, geometry.size.width - 48)),
                    height: min(820, max(0, geometry.size.height - 48))
                )
                .clipShape(RoundedRectangle(cornerRadius: 12))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}
