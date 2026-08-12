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
                        .font(.system(size: 15, weight: .medium))
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
        .background {
            LinearGradient(
                colors: [Theme.surfaceElevated, Theme.surface],
                startPoint: .top,
                endPoint: .bottom
            )
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(Color.white.opacity(0.08))
                    .frame(height: 1)
            }
        }
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
            let query = searchText
            for await result in library.searchResults(query) {
                guard !Task.isCancelled else { return }
                switch result {
                case .success(let results):
                    uiStore.searchResults = SearchResults(
                        bridge: results,
                        query: query
                    )
                case .failure(let error):
                    guard let line = error.displayLine else { continue }
                    uiStore.showError(
                        String(localized: "Search failed: \(line)")
                    )
                }
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
        // Refocusing the field reopens a dropdown that was dismissed (Escape,
        // click-away) while a query and its results are still present.
        .onChange(of: searchFocused) { _, focused in
            if focused, !searchText.isEmpty, uiStore.searchResults != nil {
                uiStore.showSearchPopover = true
            }
        }
    }
}

/// The Library/Import selector: a pill of two segments, the active one filled
/// with the accent. Reads as a segmented group to assistive tech; the caller
/// owns the section switch (and its animation).
private struct SectionSegmentedControl: View {
    let selection: MainSection
    let onSelect: (MainSection) -> Void

    var body: some View {
        HStack(spacing: 0) {
            segment("Library", section: .library)
            segment("Import", section: .importing)
        }
        .padding(4)
        .background(
            RoundedRectangle(cornerRadius: 11)
                .fill(Theme.field)
                .overlay(
                    RoundedRectangle(cornerRadius: 11)
                        .strokeBorder(Color.white.opacity(0.06), lineWidth: 1)
                )
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
                .font(.system(size: 14.5, weight: .bold))
                .foregroundStyle(active ? Color.white : Color.secondary)
                .padding(.horizontal, 20)
                .padding(.vertical, 7)
                .background(
                    RoundedRectangle(cornerRadius: 8)
                        .fill(active ? Theme.accent : Color.clear)
                        .shadow(
                            color: active ? Theme.accent.opacity(0.4) : .clear,
                            radius: active ? 6 : 0,
                            y: active ? 2 : 0
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
        TitleBarPreview()
            .environment(Library.stub())
            .environment(UiStore())
    }
#endif
