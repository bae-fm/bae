import BaeKit
import SwiftUI

/// Shared cover browser. Preview focus is separate from the saved selection.
/// The view draws the content, filter and focused cover that its caller
/// passes in, and counts or finds nothing. Their owner reads the content for
/// that filter and focus, and the content says which cover is focused and
/// where it sits.
struct MacCoverGallery: View {
    let content: CoverGalleryContent
    /// The state of the catalog lookup. The view draws remote covers from
    /// `content`, not from this value.
    let remoteItems: RemoteCoverItems
    @Binding
    var filter: ArtworkBrowserState.Filter
    /// The cover the person focused last, which the content focuses while it
    /// shows it.
    @Binding
    var focusedCover: BridgeCoverKey?
    let selectedCover: BridgeCoverSelection?
    /// The lightbox's image as it loads, owned by whoever shows the gallery.
    let lightboxLoad: LightboxImageLoad
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
            MacModalHeader(title: "Change cover", large: true) {
                if let onRefresh, remoteItems.canRefresh {
                    MacButton("Refresh", action: onRefresh)
                        .disabled(remoteItems.isLoading || isSaving)
                }
                MacButton("Done", action: onDone)
                    .keyboardShortcut(.cancelAction)
                    .disabled(isSaving)
            }
            Divider()
            HStack(spacing: 0) {
                ScrollView {
                    VStack(alignment: .leading, spacing: ThemeSpace.section) {
                        MacRow("Source") {
                            MacPopUpButton(
                                "Source",
                                selection: $filter
                            ) {
                                for filter in ArtworkBrowserState.Filter
                                    .allCases
                                {
                                    MacPopUpOption(
                                        verbatim: filter.label,
                                        value: filter
                                    )
                                }
                            }
                        }
                        if let cover = content.current {
                            section(
                                "Current cover",
                                icon: "photo",
                                items: [cover],
                                count: 1
                            )
                        }
                        if let remote = content.remote {
                            // Unlinked, there is nothing to count or list:
                            // the block that says so stands in the section.
                            if remoteItems != .unlinked {
                                section(
                                    "Remote sources",
                                    icon: "globe",
                                    items: remote.items,
                                    count: remote.count
                                )
                            }
                            remoteStatus
                        }
                        if let files = content.files {
                            section(
                                "Release files",
                                icon: "folder",
                                items: files.items,
                                count: files.count
                            )
                        }
                    }
                    .padding(ThemeSpace.section)
                }
                .frame(maxWidth: .infinity)
                Divider()
                MacCoverPreview(
                    content: content,
                    isSaving: isSaving,
                    previewFocused: $previewFocused,
                    onOpenLightbox: { browser.layout = .lightbox },
                    onFocus: { self.focus($0) }
                )
                .frame(width: 360)
                .frame(maxHeight: .infinity, alignment: .top)
                .padding(ThemeSpace.section)
                .background(Theme.well)
            }
            Divider()
            HStack(spacing: ThemeSpace.edge) {
                if let failure = error ?? remoteItems.failure {
                    MacNotice(error: failure)
                }
                Spacer(minLength: ThemeSpace.group)
                // Saving can download a remote cover, or read a release's
                // image from its storage.
                MacSpinner(
                    Text(
                        "Saving the cover…",
                        comment:
                            "Cover picker, while bae saves the cover picked, which can mean downloading it from a catalog or reading it from the release's storage."
                    )
                )
                .opacity(isSaving ? 1 : 0)
                .accessibilityHidden(!isSaving)
                MacButton("Use this cover", kind: .primary) {
                    if let focus = content.focus {
                        onSelect(CoverItem(gallery: focus.cover))
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(
                    content.focus?.cover.selection == nil
                        || content.focus?.cover.selection == selectedCover
                        || isSaving
                )
            }
            .padding(.vertical, ThemeSpace.edge)
            .padding(.horizontal, ThemeSpace.section)
        }
    }

    var body: some View {
        gallery
            .disabled(browser.layout == .lightbox)
            .accessibilityHidden(browser.layout == .lightbox)
            .overlay { lightboxOverlay }
            // The grid's preview gets keyboard focus whenever it appears: when
            // the sheet opens on the grid, its covers arrive, or the lightbox
            // closes back to it. The code never clears the preview's focus
            // itself: the lightbox gets focus when it opens, and clearing the
            // preview's focus in the same update would clear the lightbox's
            // too.
            .onChange(of: gridPreviewShown, initial: true) { _, shown in
                if shown { previewFocused = true }
            }
            .onKeyPress(.leftArrow) {
                guard browser.layout == .grid else { return .ignored }
                focus(content.focus?.neighbors?.previous)
                return .handled
            }
            .onKeyPress(.rightArrow) {
                guard browser.layout == .grid else { return .ignored }
                focus(content.focus?.neighbors?.next)
                return .handled
            }
    }

    @ViewBuilder
    private var lightboxOverlay: some View {
        if browser.layout == .lightbox {
            VStack(spacing: 0) {
                if let focus = content.focus {
                    MacLightbox(
                        place: LightboxPlace(
                            items: content.items,
                            current: CoverItem(gallery: focus.cover),
                            neighbors: focus.neighbors.map {
                                LightboxNeighbors(
                                    previous: $0.previous,
                                    next: $0.next
                                )
                            }
                        ),
                        load: lightboxLoad,
                        onSelect: { focusedCover = $0 },
                        onDismiss: showGrid,
                        onBrowseAll: showGrid
                    )
                }
                else {
                    if remoteItems.isLoading {
                        MacSpinner("Fetching covers…")
                            .frame(
                                maxWidth: .infinity,
                                maxHeight: .infinity
                            )
                    }
                    else {
                        MacContentUnavailable(title: "No cover art available")
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
        }
    }

    private var gridPreviewShown: Bool {
        browser.layout == .grid && content.focus != nil
    }

    /// Focus the cover `key` names, if any.
    private func focus(_ key: BridgeCoverKey?) {
        if let key { focusedCover = key }
    }

    private func showGrid() {
        browser.layout = .grid
    }

    @ViewBuilder
    private var lightboxStatus: some View {
        VStack(spacing: ThemeSpace.related) {
            if content.focus != nil || !remoteItems.isLoading { remoteStatus }
            if let error {
                MacNotice(error: error)
            }
            if let failure = remoteItems.failure {
                MacNotice(
                    error: failure,
                    action: onRefresh.flatMap {
                        failure.retryAction(perform: $0)
                    }
                )
            }
        }
        .themeText(.body)
        .padding(ThemeSpace.group)
    }
}

extension MacCoverGallery {
    private func section(
        _ title: LocalizedStringKey,
        icon: String,
        items: [CoverItem],
        count: Int
    ) -> some View {
        VStack(
            alignment: .leading,
            spacing: ThemeSpace.related + ThemeSpace.line
        ) {
            HStack(spacing: ThemeSpace.compact) {
                Label(title, systemImage: icon)
                    .themeText(.heading)
                Text(count, format: .number)
                    .themeText(.heading)
                    .fontWeight(.regular)
                    .monospacedDigit()
                    .foregroundStyle(Theme.textSecondary)
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
        MacCoverTile(
            item: item,
            selected: content.focus?.cover.key == item.id,
            onFocus: { focusedCover = item.id },
            onOpenLightbox: { browser.layout = .lightbox }
        )
        .disabled(isSaving)
    }
}

extension MacCoverGallery {
    @ViewBuilder
    private var remoteStatus: some View {
        switch remoteItems {
        case .loading:
            MacSpinner("Fetching covers…")
        case .unlinked:
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                Text("No linked release")
                    .themeText(.heading)
                Text(
                    "Link a Discogs or MusicBrainz release to browse its artwork."
                )
                .foregroundStyle(Theme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
                if let onFindRelease {
                    MacButton("Find release…", action: onFindRelease)
                        .disabled(isSaving)
                }
            }
        case .linked(let items) where items.isEmpty:
            Text("No remote covers found")
                .foregroundStyle(Theme.textSecondary)
        case .linked, .failed:
            EmptyView()
        }
    }
}
