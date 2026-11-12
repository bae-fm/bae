import BaeKit
import SwiftUI

/// A scrolling list whose chosen row is drawn in the accent the person chose,
/// as the board's selected file row is: the accent's tint behind the row, its
/// text and glyphs keeping their own colours. A native `List`'s selection is
/// drawn in the Mac's accent, which bae never shows, and turns the row's text
/// white. Clicking a row chooses it; the up and down arrows move the choice
/// while the list has the keyboard focus, and VoiceOver reads the chosen row
/// as selected.
struct AccentSelectionList<Row, ID: Hashable, Content: View>: View {
    let rows: [Row]
    let id: KeyPath<Row, ID>
    @Binding
    var selection: ID?
    @ViewBuilder
    let content: (Row) -> Content

    @FocusState
    private var focused: Bool

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(rows, id: id) { row in
                        line(row)
                    }
                }
            }
            .focusable()
            .focused($focused)
            .focusEffectDisabled()
            .onKeyPress(.upArrow) { move(by: -1, proxy) }
            .onKeyPress(.downArrow) { move(by: 1, proxy) }
        }
    }

    private func line(_ row: Row) -> some View {
        let rowID = row[keyPath: id]
        let chosen = selection == rowID
        return Button {
            selection = rowID
            focused = true
        } label: {
            content(row)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(chosen ? Theme.accentSoft : Color.clear)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .focusable(false)
        .id(rowID)
        .accessibilityAddTraits(chosen ? .isSelected : [])
    }

    /// Moves the choice `step` rows down, stopping at the ends; with none
    /// chosen, an arrow chooses the row at the end it points from.
    private func move(by step: Int, _ proxy: ScrollViewProxy)
        -> KeyPress.Result
    {
        guard !rows.isEmpty else { return .ignored }
        let current = rows.firstIndex { $0[keyPath: id] == selection }
        let target =
            current.map { min(max($0 + step, 0), rows.count - 1) }
            ?? (step > 0 ? 0 : rows.count - 1)
        let targetID = rows[target][keyPath: id]
        selection = targetID
        proxy.scrollTo(targetID)
        return .handled
    }
}
