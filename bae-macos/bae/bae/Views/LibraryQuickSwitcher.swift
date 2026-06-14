import SwiftUI

/// Quick library switcher overlay: a small floating panel that lists
/// local libraries, supports type-to-filter, and switches on Enter.
/// Opened from the "Switch Library..." File-menu item.
struct LibraryQuickSwitcher: View {
    let libraries: [BridgeLibrary]
    let onPick: (BridgeLibrary) -> Void
    let onCancel: () -> Void

    @State
    private var query: String = ""
    @State
    private var highlightedIndex: Int = 0
    @FocusState
    private var fieldFocused: Bool

    private var matches: [BridgeLibrary] {
        let trimmed = query.trimmingCharacters(in: .whitespaces).lowercased()
        if trimmed.isEmpty { return libraries }
        return libraries.filter {
            $0.name.lowercased().contains(trimmed)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            TextField("Switch to library...", text: $query)
                .textFieldStyle(.plain)
                .font(.title3)
                .padding(12)
                .focused($fieldFocused)
                .onSubmit(commitHighlighted)

            Divider()

            if matches.isEmpty {
                Text("No libraries match.")
                    .foregroundStyle(.secondary)
                    .font(.callout)
                    .padding(12)
            }
            else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(matches.enumerated()), id: \.offset) {
                            idx,
                            lib in
                            row(
                                for: lib,
                                isHighlighted: idx == highlightedIndex
                            )
                            .onTapGesture {
                                onPick(lib)
                            }
                        }
                    }
                }
                .frame(maxHeight: 240)
            }
        }
        .frame(width: 420)
        .background(.regularMaterial)
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay {
            RoundedRectangle(cornerRadius: 10)
                .strokeBorder(
                    Color.gray.opacity(0.25),
                    lineWidth: 1
                )
        }
        .padding(40)
        .background(KeyEventCatcher(onKey: handleKey))
        .onAppear {
            fieldFocused = true
            highlightedIndex = 0
        }
        .onChange(of: matches.count) { _, _ in
            highlightedIndex = 0
        }
    }

    @ViewBuilder
    private func row(for lib: BridgeLibrary, isHighlighted: Bool) -> some View {
        HStack {
            Text(lib.name)
                .lineLimit(1)
            Spacer()
            if lib.isActive {
                Text("Active")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(
            isHighlighted
                ? Color.accentColor.opacity(0.2) : Color.clear
        )
        .contentShape(Rectangle())
    }

    private func handleKey(_ key: KeyEquivalent) -> Bool {
        switch key {
        case .escape:
            onCancel()
            return true
        case .downArrow:
            if !matches.isEmpty {
                highlightedIndex = min(
                    highlightedIndex + 1,
                    matches.count - 1
                )
            }
            return true
        case .upArrow:
            if !matches.isEmpty {
                highlightedIndex = max(highlightedIndex - 1, 0)
            }
            return true
        default:
            return false
        }
    }

    private func commitHighlighted() {
        guard !matches.isEmpty else { return }
        let idx = min(highlightedIndex, matches.count - 1)
        onPick(matches[idx])
    }
}

/// Tiny NSViewRepresentable that swallows arrow keys / Escape from the
/// focused TextField inside the overlay. SwiftUI's TextField consumes
/// arrow keys for text navigation, so the only way to get arrow-based
/// list movement is to intercept the key events at the AppKit layer.
private struct KeyEventCatcher: NSViewRepresentable {
    let onKey: (KeyEquivalent) -> Bool

    func makeNSView(context: Context) -> NSView {
        let view = KeyView()
        view.onKey = onKey
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        (nsView as? KeyView)?.onKey = onKey
    }

    private final class KeyView: NSView {
        var onKey: ((KeyEquivalent) -> Bool)?

        override var acceptsFirstResponder: Bool { false }

        override func keyDown(with event: NSEvent) {
            let key: KeyEquivalent? =
                switch event.keyCode {
                case 53: .escape
                case 125: .downArrow
                case 126: .upArrow
                default: nil
                }
            if let key, onKey?(key) == true {
                return
            }
            super.keyDown(with: event)
        }
    }
}
