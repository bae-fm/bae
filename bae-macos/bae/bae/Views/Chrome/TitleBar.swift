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

/// The Library/Import selector; the caller owns the section switch and its
/// animation.
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
            RoundedRectangle(cornerRadius: ThemeRadius.control)
                .fill(Theme.well)
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
                .themeText(.strong)
                .foregroundStyle(active ? Color.primary : Color.secondary)
                .padding(.horizontal, 18)
                .padding(.vertical, 6)
                .background(
                    RoundedRectangle(cornerRadius: ThemeRadius.control - 3)
                        .fill(Theme.tile)
                        .opacity(active ? 1 : 0)
                        .shadow(
                            color: active ? Theme.shadow : Color.clear,
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

#if DEBUG
    // MARK: - Previews

    /// Owns the search text the title bar binds to.
    private struct TitleBarPreview: View {
        @State
        private var searchText = ""

        var body: some View {
            TitleBar(searchText: $searchText)
                .frame(width: 1100)
        }
    }

    // The environment sits on the #Preview root because the missing-environment
    // audit reads only the preview closure's modifier chain.
    #Preview("Title bar") {
        let library = Library.stub()
        TitleBarPreview()
            .environment(library)
            .environment(LibraryProjectionStore(library: library))
            .environment(UiStore())
    }
#endif
