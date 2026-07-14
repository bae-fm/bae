import BaeKit
import SwiftUI

extension FocusedValues {
    @Entry
    var focusSearch: (() -> Void)?
}

private let titleBarLeadingPadding: CGFloat = 80
private let titleBarTrailingPadding: CGFloat = 16

struct TitleBar: View {
    @Environment(Library.self)
    var library
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
            HStack(spacing: 8) {
                Picker(
                    "Section",
                    selection: Binding(
                        get: { uiStore.activeSection },
                        set: { newValue in
                            withAnimation(.spring(duration: 0.2, bounce: 0.15))
                            {
                                uiStore.switchSection(newValue)
                            }
                        },
                    )
                ) {
                    Text("Library").tag(MainSection.library)
                    Text("Import").tag(MainSection.importing)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .frame(width: 200)
            }
            .offset(x: -(titleBarLeadingPadding - titleBarTrailingPadding) / 2)

            HStack(spacing: 12) {
                Spacer()
                LibrarySearchField(
                    text: $searchText,
                    prompt: "Artists, albums, tracks",
                    focused: $searchFocused,
                    onEscape: {
                        searchFocused = false
                        uiStore.showSearchPopover = false
                    }
                )
                .frame(width: 250)
                .anchorPreference(
                    key: SearchFieldAnchorKey.self,
                    value: .bounds
                ) { $0 }

                Button(action: { openSettings() }) {
                    Image(systemName: "gearshape")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .help("Settings")
            }
        }
        .padding(.top, 8)
        .padding(.bottom, 8)
        .padding(.leading, titleBarLeadingPadding)
        .padding(.trailing, titleBarTrailingPadding)
        .background { WindowDragArea() }
        .background(Theme.surface)
        .task(id: searchText) {
            if searchText.isEmpty {
                uiStore.showSearchPopover = false
                uiStore.searchResults = nil
                return
            }
            do {
                try await Task.sleep(for: .milliseconds(300))
            }
            catch {
                return
            }
            do {
                let results = try await library.searchLibrary(searchText)
                guard !Task.isCancelled else { return }
                uiStore.searchResults = SearchResults(
                    bridge: results,
                    query: searchText
                )
            }
            catch {
                guard !Task.isCancelled, let line = error.displayLine else {
                    return
                }
                uiStore.showError(
                    String(localized: "Search failed: \(line)")
                )
            }
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
    }
}
