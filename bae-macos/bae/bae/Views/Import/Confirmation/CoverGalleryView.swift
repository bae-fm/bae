import BaeKit
import SwiftUI

/// Shared cover browser. Preview focus is separate from the saved selection.
struct CoverGalleryView: View {
    let remoteItems: RemoteCoverItems
    let releaseItems: [CoverItem]
    let selectedCover: BridgeCoverSelection?
    /// The lightbox's image as it loads, owned by whoever shows the gallery.
    let lightboxLoad: LightboxImageLoad
    var currentCover: CoverItem?
    var initialLayout: ArtworkBrowserState.Layout = .grid
    var isSaving = false
    var error: DisplayError?
    var onRefresh: (() -> Void)?
    var onFindRelease: (() -> Void)?
    let onSelect: (CoverItem) -> Void
    let onDone: () -> Void

    @State
    private var browser = ArtworkBrowserState(layout: .grid)
    @FocusState
    private var previewFocused: Bool

    private var gallery: some View {
        VStack(spacing: 0) {
            HStack(spacing: ThemeSpace.group) {
                Text(initialLayout == .lightbox ? "Images" : "Change cover")
                    .themeText(.title)
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
            .padding(ThemeSpace.section)
            Divider()
            HStack(spacing: 0) {
                ScrollView {
                    VStack(alignment: .leading, spacing: ThemeSpace.section) {
                        Picker(
                            "Source",
                            selection: Binding(
                                get: { browser.filter },
                                set: { browser.setFilter($0) }
                            )
                        ) {
                            ForEach(
                                ArtworkBrowserState.Filter.allCases,
                                id: \.self
                            ) { filter in
                                Text(verbatim: filter.label).tag(filter)
                            }
                        }
                        .pickerStyle(.menu)
                        if let cover = browser.currentCover {
                            section(
                                "Current cover",
                                icon: "photo",
                                items: [cover]
                            )
                        }
                        if browser.showsRemoteSources {
                            section(
                                "Remote sources",
                                icon: "globe",
                                items: browser.remoteItems
                            )
                            remoteStatus
                        }
                        if browser.showsReleaseFiles {
                            section(
                                "Release files",
                                icon: "folder",
                                items: browser.releaseItems
                            )
                        }
                    }
                    .padding(ThemeSpace.section)
                }
                .frame(maxWidth: .infinity)
                Divider()
                preview
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .padding(ThemeSpace.section)
                    .background(Theme.well)
            }
            Divider()
            HStack(spacing: ThemeSpace.edge) {
                if let failure = error ?? remoteItems.failure {
                    ErrorNotice(error: failure)
                }
                Spacer(minLength: ThemeSpace.group)
                // Saving can download a remote cover, or read a release's
                // image from its storage.
                WaitingIndicator(
                    Text(
                        "Saving the cover…",
                        comment:
                            "Cover picker, while bae saves the cover picked, which can mean downloading it from a catalog or reading it from the release's storage."
                    )
                )
                .opacity(isSaving ? 1 : 0)
                .accessibilityHidden(!isSaving)
                Button("Use this cover") {
                    if let cursor = browser.cursor { onSelect(cursor.current) }
                }
                .buttonStyle(PrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
                .disabled(
                    browser.cursor?.current.selection == nil
                        || browser.cursor?.current.selection == selectedCover
                        || isSaving
                )
            }
            .padding(ThemeSpace.section)
        }
        .background(Theme.background)
    }

    var body: some View {
        Group {
            if initialLayout == .lightbox {
                CoverPickerFrame { gallery }
            }
            else {
                gallery
            }
        }
        .disabled(browser.layout == .lightbox)
        .accessibilityHidden(browser.layout == .lightbox)
        .overlay {
            if browser.layout == .lightbox {
                VStack(spacing: 0) {
                    if let cursor = browser.cursor {
                        LightboxView(
                            cursor: cursor,
                            load: lightboxLoad,
                            onUpdate: { browser.cursor = $0 },
                            onDismiss: {
                                if initialLayout == .lightbox {
                                    onDone()
                                }
                                else {
                                    showGrid()
                                }
                            },
                            onBrowseAll: showGrid
                        )
                    }
                    else {
                        if remoteItems.isLoading {
                            WaitingIndicator("Fetching covers…")
                                .frame(
                                    maxWidth: .infinity,
                                    maxHeight: .infinity
                                )
                        }
                        else {
                            ContentUnavailableView {
                                Text("No cover art available")
                            }
                        }
                        Button("Browse all images", action: showGrid)
                        Button("Done", action: onDone)
                            .keyboardShortcut(.cancelAction)
                    }
                    if remoteItems.hasStatus || error != nil {
                        lightboxStatus
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Theme.background)
            }
        }
        .onAppear {
            browser.layout = initialLayout
            rebuild()
        }
        .onChange(of: remoteItems) { _, _ in rebuild() }
        .onChange(of: releaseItems) { _, _ in rebuild() }
        .onChange(of: currentCover?.image) { _, _ in rebuild() }
        // The grid's preview takes the keys whenever it comes on screen: when
        // the sheet opens on the grid, its covers arrive, or the lightbox
        // closes back to it. It never gives them up by hand: the lightbox
        // takes them as it opens, and clearing the preview's focus in the
        // same update would clear the lightbox's too.
        .onChange(of: gridPreviewShown, initial: true) { _, shown in
            if shown { previewFocused = true }
        }
        .onKeyPress(.leftArrow) {
            guard browser.layout == .grid else { return .ignored }
            browser.cursor?.goToPrevious()
            return .handled
        }
        .onKeyPress(.rightArrow) {
            guard browser.layout == .grid else { return .ignored }
            browser.cursor?.goToNext()
            return .handled
        }
    }

    private var gridPreviewShown: Bool {
        browser.layout == .grid && browser.cursor != nil
    }

    private func rebuild() {
        browser.update(
            currentCover: currentCover,
            remoteItems: remoteItems.items,
            releaseItems: releaseItems,
            selectedCover: selectedCover
        )
    }

    private func showGrid() {
        browser.layout = .grid
    }

    @ViewBuilder
    private var lightboxStatus: some View {
        VStack(spacing: ThemeSpace.related) {
            if browser.cursor != nil || !remoteItems.isLoading { remoteStatus }
            if let error {
                ErrorNotice(error: error)
            }
            if let failure = remoteItems.failure {
                ErrorNotice(
                    error: failure,
                    action: onRefresh.map { ErrorAction(perform: $0) }
                )
            }
        }
        .themeText(.body)
        .padding(ThemeSpace.group)
    }

}

extension CoverGalleryView {
    private func section(
        _ title: LocalizedStringKey,
        icon: String,
        items: [CoverItem]
    ) -> some View {
        VStack(alignment: .leading, spacing: ThemeSpace.edge) {
            HStack {
                Label(title, systemImage: icon)
                    .themeText(.heading)
                Spacer()
                Text(items.count, format: .number)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
            LazyVGrid(
                columns: [
                    GridItem(
                        .adaptive(minimum: 130),
                        spacing: ThemeSpace.edge
                    )
                ],
                spacing: ThemeSpace.section
            ) {
                ForEach(items) { item in
                    tile(for: item)
                }
            }
        }
    }

    private func tile(for item: CoverItem) -> some View {
        Button {
            browser.cursor?.select(id: item.id)
        } label: {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                ImageView(
                    content: item.image,
                    contentMode: .fit,
                    pointSize: 180
                )
                .frame(height: 138)
                .frame(maxWidth: .infinity)
                .padding(ThemeSpace.related)
                .background(Theme.hover)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
                .overlay {
                    RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                        .strokeBorder(
                            browser.cursor?.current.id == item.id
                                ? Color.accentColor : .clear,
                            lineWidth: 2
                        )
                }
                .overlay(alignment: .topTrailing) {
                    Image(systemName: "checkmark.circle.fill")
                        .symbolRenderingMode(.palette)
                        .foregroundStyle(Theme.onFill, Theme.accent)
                        .padding(ThemeSpace.related)
                        .opacity(
                            item.id == .currentCover
                                || item.selection == selectedCover ? 1 : 0
                        )
                }
                Text(verbatim: item.label)
                    .themeText(.rowTitle)
                    .lineLimit(2)
                    .truncationMode(.middle)
                    .frame(height: 34, alignment: .topLeading)
                Text(verbatim: item.sourceLabel)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(item.label)
        .accessibilityLabel(item.label)
        .accessibilityAddTraits(
            browser.cursor?.current.id == item.id ? .isSelected : []
        )
        .disabled(isSaving)
        .simultaneousGesture(
            TapGesture(count: 2)
                .onEnded {
                    browser.cursor?.select(id: item.id)
                    browser.layout = .lightbox
                }
        )
    }

}

extension CoverGalleryView {
    @ViewBuilder
    private var remoteStatus: some View {
        switch remoteItems {
        case .loading:
            WaitingIndicator("Fetching covers…")
        case .unlinked:
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                Text("No linked release")
                    .themeText(.heading)
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
        if let cursor = browser.cursor {
            VStack(spacing: ThemeSpace.section) {
                Button {
                    browser.layout = .lightbox
                } label: {
                    ImageView(
                        content: cursor.current.image,
                        contentMode: .fit,
                        pointSize: 800
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .clipped()
                    .contentShape(Rectangle())
                    .overlay(alignment: .topTrailing) {
                        Image(systemName: "arrow.up.left.and.arrow.down.right")
                            .padding(ThemeSpace.related)
                            .background(.regularMaterial, in: Circle())
                            .padding(ThemeSpace.related)
                    }
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("View in lightbox"))
                .help("View in lightbox")
                .keyboardShortcut(.space, modifiers: [])
                .focusable()
                .focusEffectDisabled()
                .focused($previewFocused)
                .disabled(isSaving)
                Text(verbatim: cursor.current.label)
                    .themeText(.heading)
                    .multilineTextAlignment(.center)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                Text(verbatim: cursor.current.sourceLabel)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                HStack(spacing: ThemeSpace.section) {
                    Button {
                        browser.cursor?.goToPrevious()
                    } label: {
                        Image(systemName: "chevron.left")
                    }
                    .accessibilityLabel(Text("Previous image"))
                    Text("\(cursor.index + 1) of \(cursor.items.count)")
                        .monospacedDigit().foregroundStyle(.secondary)
                    Button {
                        browser.cursor?.goToNext()
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
            ContentUnavailableView {
                Text("No cover art available")
            }
        }
    }
}

/// Bounds cover browsers to the available modal host, including short windows.
struct CoverPickerFrame<Content: View>: View {
    /// The space left around the browser, both sides together.
    private let margins = 2 * ThemeSpace.section

    @ViewBuilder
    let content: () -> Content

    var body: some View {
        GeometryReader { geometry in
            content()
                .frame(
                    width: min(1_100, max(0, geometry.size.width - margins)),
                    height: min(820, max(0, geometry.size.height - margins))
                )
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.card))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}
