import BaeKit
import SwiftUI

extension FocusedValues {
    @Entry
    var focusSearch: (() -> Void)?
}

private let titleBarLeadingPadding: CGFloat = 80
private let titleBarTrailingPadding: CGFloat = 16

struct TitleBar: View {
    @Environment(LibraryProjectionStore.self)
    private var libraryProjections
    @Environment(UiStore.self)
    var uiStore
    @Environment(\.openSettings)
    private var openSettings
    @Binding
    var searchText: String
    @FocusState
    private var searchFocused: Bool
    var body: some View {
        ZStack {
            SectionSegmentedControl(
                selection: uiStore.activeSection,
                onSelect: { newValue in
                    withAnimation(.spring(duration: 0.2, bounce: 0.15)) {
                        uiStore.switchSection(newValue)
                    }
                }
            )
            .offset(x: -(titleBarLeadingPadding - titleBarTrailingPadding) / 2)

            HStack(spacing: 12) {
                Spacer()
                LibrarySearchField(
                    text: $searchText,
                    prompt: "Search",
                    focused: $searchFocused,
                    onEscape: {
                        searchFocused = false
                        uiStore.showSearchPopover = false
                    }
                )
                .frame(width: 300)
                .anchorPreference(
                    key: SearchFieldAnchorKey.self,
                    value: .bounds
                ) { $0 }

                Button(action: { openSettings() }) {
                    Image(systemName: "gearshape")
                        .font(.system(size: 17, weight: .medium))
                        .frame(width: 34, height: 34)
                        .contentShape(Rectangle())
                }
                .buttonStyle(IconHoverButtonStyle())
                .help("Settings")
            }
        }
        .padding(.leading, titleBarLeadingPadding)
        .padding(.trailing, titleBarTrailingPadding)
        .frame(height: 56)
        .background { WindowDragArea() }
        // The bar is the window's own ground with a hairline under it, not a
        // raised band: the controls on it are what set it apart.
        .background {
            Theme.background
                .overlay(alignment: .bottom) {
                    Rectangle()
                        .fill(Color.white.opacity(0.07))
                        .frame(height: 1)
                }
        }
        .onChange(of: searchText, initial: true) { oldValue, newValue in
            libraryProjections.deactivateSearch(oldValue)
            if searchText.isEmpty {
                uiStore.showSearchPopover = false
                uiStore.searchResults = nil
                return
            }
            libraryProjections.activateSearch(newValue)
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
            libraryProjections.deactivateSearch(searchText)
        }
        .focusedSceneValue(\.focusSearch) { searchFocused = true }
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

/// The Library/Import selector: a sunken pill of two segments, the active one
/// raised on a neutral tile. Reads as a segmented group to assistive tech; the
/// caller owns the section switch (and its animation).
private struct SectionSegmentedControl: View {
    let selection: MainSection
    let onSelect: (MainSection) -> Void

    var body: some View {
        HStack(spacing: 0) {
            segment("Library", section: .library)
            segment("Import", section: .importing)
        }
        .padding(3)
        .background(
            RoundedRectangle(cornerRadius: 9)
                .fill(TitleBarChrome.well)
        )
        .accessibilityElement(children: .contain)
    }

    private func segment(
        _ title: LocalizedStringKey,
        section: MainSection
    ) -> some View {
        let active = selection == section
        return Button {
            onSelect(section)
        } label: {
            Text(title)
                .font(.system(size: 13.5, weight: .semibold))
                .foregroundStyle(active ? Color.primary : Color.secondary)
                .padding(.horizontal, 18)
                .padding(.vertical, 6)
                .background(
                    RoundedRectangle(cornerRadius: 6.5)
                        .fill(active ? TitleBarChrome.tile : Color.clear)
                        .shadow(
                            color: .black.opacity(active ? 0.45 : 0),
                            radius: 1.5,
                            y: 1
                        )
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .animation(.easeInOut(duration: 0.15), value: active)
        .accessibilityAddTraits(
            active ? [.isButton, .isSelected] : .isButton
        )
    }
}

/// The title bar's two fills: the sunken well the segmented control and the
/// search field sit in, and the raised tile the active segment stands on.
enum TitleBarChrome {
    static let well = Color.black.opacity(0.28)
    static let tile = Color(red: 0.165, green: 0.165, blue: 0.192)
}

#if DEBUG
    // MARK: - Previews

    /// Owns the search text the title bar binds to and injects the two services
    /// it reads from the environment as stubs. An empty query keeps the search
    /// task's debounce from firing.
    private struct TitleBarPreview: View {
        @State
        private var searchText = ""

        var body: some View {
            TitleBar(searchText: $searchText)
                .frame(width: 1100)
        }
    }

    // The environment lives on the #Preview root (not inside TitleBarPreview's
    // body) so the missing-environment audit, which only reads the preview
    // closure's modifier chain, can see it.
    #Preview("Title bar") {
        let library = Library.stub()
        TitleBarPreview()
            .environment(library)
            .environment(LibraryProjectionStore(library: library))
            .environment(UiStore())
    }
#endif
