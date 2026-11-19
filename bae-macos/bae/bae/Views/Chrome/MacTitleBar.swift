import BaeKit
import Combine
import SwiftUI

extension EnvironmentValues {
    /// View ▸ Search's requests to focus the window's search field.
    @Entry
    var searchFocusRequests = PassthroughSubject<Void, Never>()
}

private let titleBarLeadingPadding: CGFloat = 80
private let titleBarTrailingPadding = ThemeSpace.edge

struct MacTitleBar: View {
    /// The bar's height, which the search card below it is hung from.
    static let height: CGFloat = 56

    @Environment(LibraryProjectionStore.self)
    private var libraryProjections
    @Environment(UiStore.self)
    var uiStore
    @Environment(\.openSettingsWindow)
    private var openSettings
    @Binding
    var searchText: String
    /// Told the search field's frame in `SearchOverlaySpace` whenever it
    /// moves, so the search dropdown can hang under it.
    let onSearchFieldFrame: (CGRect) -> Void
    @Environment(\.searchFocusRequests)
    private var searchFocusRequests
    @FocusState
    private var searchFocused: Bool
    var body: some View {
        ZStack {
            MacSectionTabs(
                selection: uiStore.activeSection,
                onSelect: { newValue in
                    withAnimation(.spring(duration: 0.2, bounce: 0.15)) {
                        uiStore.switchSection(newValue)
                    }
                }
            )
            .padding(
                .trailing,
                titleBarLeadingPadding - titleBarTrailingPadding
            )

            HStack(spacing: ThemeSpace.group) {
                Spacer()
                MacSearchField(
                    text: $searchText,
                    prompt: "Search",
                    focused: $searchFocused,
                    onEscape: {
                        searchFocused = false
                        uiStore.showSearchPopover = false
                    }
                )
                .frame(width: 300)
                .onGeometryChange(for: CGRect.self) { geometry in
                    geometry.frame(in: .named(SearchOverlaySpace.name))
                } action: { frame in
                    onSearchFieldFrame(frame)
                }

                MacActivityButton()

                Button(action: { openSettings() }) {
                    Image(systemName: "gearshape")
                        .themeIcon(.large)
                        .frame(
                            width: ThemeSize.hitTarget,
                            height: ThemeSize.hitTarget
                        )
                        .contentShape(Rectangle())
                }
                .buttonStyle(IconHoverButtonStyle().accentFocus())
                .help("Settings")
                .accessibilityLabel("Settings")
            }
        }
        .padding(.leading, titleBarLeadingPadding)
        .padding(.trailing, titleBarTrailingPadding)
        .frame(height: Self.height)
        .background { WindowDragArea() }
        // The window's own background with a hairline under it, not a raised
        // band.
        .background {
            Rectangle().fill(Theme.background)
                .overlay(alignment: .bottom) {
                    Rectangle()
                        .fill(Theme.hairline)
                        .frame(height: 1)
                }
        }
        .onChange(of: searchText, initial: true) { _, newValue in
            libraryProjections.activateSearch(newValue)
            if searchText.isEmpty {
                uiStore.showSearchPopover = false
                uiStore.searchResults = nil
            }
        }
        .onChange(of: libraryProjections.search.value) { _, results in
            if let results {
                uiStore.searchResults = results
            }
        }
        .onChange(of: libraryProjections.search.error?.line) { _, line in
            guard let line else { return }
            uiStore.showError(String(localized: "Search failed: \(line)"))
        }
        .onDisappear {
            libraryProjections.deactivateSearch()
        }
        .onReceive(searchFocusRequests) {
            searchFocused = true
        }
        .onChange(of: uiStore.searchResults != nil) { _, hasResults in
            if hasResults, !searchText.isEmpty {
                uiStore.showSearchPopover = true
            }
            else {
                uiStore.showSearchPopover = false
            }
        }
        // Refocusing the field reopens a dropdown that was dismissed (Escape,
        // click-away) while a query and its results are still present.
        .onChange(of: searchFocused) { _, focused in
            if focused, !searchText.isEmpty, uiStore.searchResults != nil {
                uiStore.showSearchPopover = true
            }
        }
    }
}
