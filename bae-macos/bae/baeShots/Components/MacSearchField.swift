import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSearchField` as a search, a filter and an inline filter.
    static let macSearchFieldScenes: [ShotScene] = [
        searchField("mac.component.search-field.search", kind: .search),
        searchField(
            "mac.component.search-field.search-typed",
            kind: .search,
            text: "Low Tide"
        ),
        searchField(
            "mac.component.search-field.search-long",
            kind: .search,
            text: "Low Tide Hours by Mara Quill and the harbor recordings"
        ),
        searchField(
            "mac.component.search-field.search-focused",
            kind: .search,
            text: "Low Tide",
            focusing: true
        ),
        searchField(
            "mac.component.search-field.filter",
            kind: .filter,
            prompt: "Filter"
        ),
        searchField(
            "mac.component.search-field.filter-typed",
            kind: .filter,
            prompt: "Filter",
            text: "Low Tide"
        ),
        searchField(
            "mac.component.search-field.filter-off",
            kind: .filter,
            prompt: "Filter",
            off: true
        ),
        searchField(
            "mac.component.search-field.inline",
            kind: .inline,
            prompt: "Filter"
        ),
        searchField(
            "mac.component.search-field.inline-typed",
            kind: .inline,
            prompt: "Filter",
            text: "Low Tide"
        ),
    ]

    private static func searchField(
        _ id: String,
        kind: MacSearchFieldKind,
        prompt: LocalizedStringKey = "Search",
        text: String = "",
        focusing: Bool = false,
        off: Bool = false
    ) -> ShotScene {
        component(id, height: 50) {
            ShotSearchField(
                kind: kind,
                prompt: prompt,
                text: text,
                focusing: focusing
            )
            // The title bar's field is 300 points wide; the inline field
            // fills a row that wide in the preview.
            .frame(
                width: kind == .filter ? nil : 300,
                alignment: .leading
            )
            .disabled(off)
        }
    }
}

/// A `MacSearchField` holding its own text and focus, for a shot.
private struct ShotSearchField: View {
    let kind: MacSearchFieldKind
    let prompt: LocalizedStringKey
    let focusing: Bool
    @State
    private var text: String
    @FocusState
    private var focused: Bool

    init(
        kind: MacSearchFieldKind,
        prompt: LocalizedStringKey,
        text: String,
        focusing: Bool
    ) {
        self.kind = kind
        self.prompt = prompt
        self.focusing = focusing
        _text = State(initialValue: text)
    }

    var body: some View {
        MacSearchField(
            kind: kind,
            text: $text,
            prompt: prompt,
            focused: $focused
        )
        .onAppear { focused = focusing }
    }
}
