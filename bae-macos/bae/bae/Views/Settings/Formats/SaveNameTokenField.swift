import AppKit
import BaeKit
import SwiftUI

/// What a name pattern's token field holds for a value: a chip.
final class SaveNameChip<Value: SaveNameValue>: NSObject {
    let value: Value

    init(_ value: Value) {
        self.value = value
    }
}

/// What the token field holds for typed text. A plain string would lose its
/// leading and trailing spaces and the field would drop a text of spaces
/// alone, so typed text is its own kind of object, kept exactly.
final class SaveNameText: NSObject {
    let text: String

    init(_ text: String) {
        self.text = text
    }
}

/// How the pane's Add row reaches the field it adds a chip to.
@MainActor
final class SaveNameFieldHandle<Piece: SaveNamePiece> {
    fileprivate weak var field: SaveNameTokenField<Piece>.Coordinator?

    func append(_ value: Piece.Value) {
        field?.append(value)
    }
}

/// A name pattern as one field you type in, like Finder's rename Format
/// field: values are chips among the text, Delete after a chip or Forward
/// Delete before it takes the chip out, and chips drag to where they belong.
/// Every change is stored as it is made, in order; the field holds what is
/// typed while it is being typed in, and takes the stored pattern otherwise.
struct SaveNameTokenField<Piece: SaveNamePiece>: NSViewRepresentable {
    typealias Chip = SaveNameChip<Piece.Value>

    /// The stored pattern.
    let pieces: [Piece]
    let handle: SaveNameFieldHandle<Piece>
    /// Whether the field is the one being typed in.
    @Binding
    var focused: Bool
    let setPieces: @MainActor ([Piece]) async throws -> Void
    let fail: @MainActor (Error) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> TokenField {
        let field = Self.makeField(coordinator: context.coordinator)
        handle.field = context.coordinator
        return field
    }

    func updateNSView(_ field: TokenField, context: Context) {
        let coordinator = context.coordinator
        coordinator.setPieces = setPieces
        coordinator.fail = fail
        coordinator.setFocused = { focused = $0 }
        field.isEnabled = context.environment.isEnabled
        coordinator.stored(pieces)
    }

    /// The proposed width, and the height its pieces take wrapped to it.
    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView field: TokenField,
        context: Context
    ) -> CGSize? {
        let width = proposal.width ?? 320
        let height =
            field.cell?
            .cellSize(
                forBounds: NSRect(x: 0, y: 0, width: width, height: 10_000)
            )
            .height ?? 0
        return CGSize(width: width, height: ceil(height))
    }

    /// A token field laid out as the pane's other fields: the chrome around
    /// it is the pane's, and what it holds is the coordinator's.
    static func makeField(coordinator: Coordinator) -> TokenField {
        let field = TokenField()
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = ThemeText.body.nsFont
        // No typed character turns text into a chip, so text stays one run
        // of text between chips.
        field.tokenizingCharacterSet = CharacterSet()
        field.cell?.wraps = true
        field.cell?.isScrollable = false
        field.lineBreakMode = .byWordWrapping
        field.delegate = coordinator
        field.target = coordinator
        field.action = #selector(Coordinator.fieldChanged(_:))
        field.onFocusChange = { [weak coordinator] focused in
            coordinator?.focus(focused)
        }
        coordinator.field = field
        return field
    }

    /// A token field that says when it becomes the one being typed in, and
    /// when it stops.
    final class TokenField: NSTokenField {
        var onFocusChange: ((Bool) -> Void)?

        /// Whether the field's editor is the window's first responder.
        var isEditing: Bool {
            guard let editor = currentEditor() else { return false }
            return window?.firstResponder === editor
        }

        override func becomeFirstResponder() -> Bool {
            let became = super.becomeFirstResponder()
            if became {
                onFocusChange?(true)
            }
            return became
        }

        override func textDidEndEditing(_ notification: Notification) {
            super.textDidEndEditing(notification)
            onFocusChange?(false)
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTokenFieldDelegate {
        fileprivate weak var field: TokenField?
        var setPieces: @MainActor ([Piece]) async throws -> Void = { _ in }
        var fail: @MainActor (Error) -> Void = { _ in }
        var setFocused: @MainActor (Bool) -> Void = { _ in }

        /// The pattern as last stored.
        private var storedPieces: [Piece] = []
        /// The last pattern handed to the store, until the store shows it:
        /// a field that shows it already isn't reset to the older pattern
        /// the store still shows.
        private var lastSubmitted: [Piece]?
        /// The newest pattern waiting for the write in flight.
        private var queued: [Piece]?
        private var writing = false

        /// Whether every pattern handed over has been stored or refused.
        var isIdle: Bool {
            !writing
        }

        private static var pasteboardType: NSPasteboard.PasteboardType {
            NSPasteboard.PasteboardType("fm.bae.save-name-chip")
        }

        func focus(_ focused: Bool) {
            setFocused(focused)
            if !focused {
                settle()
            }
        }

        /// The store shows `pieces`. A field not being typed in takes them.
        func stored(_ pieces: [Piece]) {
            storedPieces = pieces
            if pieces == lastSubmitted {
                lastSubmitted = nil
            }
            settle()
        }

        private func settle() {
            guard let field, !field.isEditing else { return }
            let shown = pieces(in: field)
            guard shown != storedPieces, shown != lastSubmitted else { return }
            field.objectValue = objects(for: storedPieces)
        }

        /// The pattern the field holds now.
        func pieces(in field: NSTokenField) -> [Piece] {
            var pieces: [Piece] = []
            var text = ""
            func endText() {
                if !text.isEmpty {
                    pieces.append(Piece(text: text))
                    text = ""
                }
            }
            for object in field.objectValue as? [Any] ?? [] {
                switch object {
                case let chip as Chip:
                    endText()
                    pieces.append(Piece(value: chip.value))
                case let typed as SaveNameText:
                    text += typed.text
                case let string as String:
                    text += string
                default:
                    break
                }
            }
            endText()
            return pieces
        }

        func objects(for pieces: [Piece]) -> [Any] {
            pieces.map { piece in
                switch piece.content {
                case .value(let value): Chip(value)
                case .text(let text): SaveNameText(text)
                }
            }
        }

        /// Put `value` as a chip at the end, after what is typed.
        func append(_ value: Piece.Value) {
            guard let field else { return }
            // Ends the editing, which hands what is typed to the store.
            field.window?.makeFirstResponder(nil)
            let pieces = pieces(in: field) + [Piece(value: value)]
            field.objectValue = objects(for: pieces)
            submit(pieces)
        }

        /// The field's pattern changed: typed into, a chip removed or moved.
        private func changed() {
            guard let field else { return }
            submit(pieces(in: field))
        }

        @objc
        func fieldChanged(_ sender: Any?) {
            changed()
        }

        func controlTextDidChange(_ notification: Notification) {
            changed()
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            changed()
        }

        /// Store `pieces` after the writes before it, one at a time, so the
        /// store ends with the last one.
        private func submit(_ pieces: [Piece]) {
            guard pieces != (lastSubmitted ?? storedPieces) else { return }
            lastSubmitted = pieces
            queued = pieces
            guard !writing else { return }
            writing = true
            Task {
                await drain()
            }
        }

        private func drain() async {
            while let next = queued {
                queued = nil
                do {
                    try await setPieces(next)
                }
                catch {
                    // The store keeps the pattern it had.
                    lastSubmitted = nil
                    fail(error)
                }
            }
            writing = false
            settle()
        }

        // MARK: Token field delegate

        func tokenField(
            _ tokenField: NSTokenField,
            displayStringForRepresentedObject representedObject: Any
        ) -> String? {
            switch representedObject {
            case let chip as Chip: chip.value.label
            case let typed as SaveNameText: typed.text
            default: representedObject as? String
            }
        }

        /// A chip is not edited as text; typed text is.
        func tokenField(
            _ tokenField: NSTokenField,
            editingStringForRepresentedObject representedObject: Any
        ) -> String? {
            (representedObject as? SaveNameText)?.text
        }

        func tokenField(
            _ tokenField: NSTokenField,
            representedObjectForEditing editingString: String
        ) -> Any? {
            SaveNameText(editingString)
        }

        /// Chips are tokens; typed text is plain text between them.
        func tokenField(
            _ tokenField: NSTokenField,
            styleForRepresentedObject representedObject: Any
        ) -> NSTokenField.TokenStyle {
            representedObject is Chip ? .rounded : .none
        }

        func tokenField(
            _ tokenField: NSTokenField,
            writeRepresentedObjects objects: [Any],
            to pasteboard: NSPasteboard
        ) -> Bool {
            let chips = objects.compactMap { $0 as? Chip }
            guard let chip = chips.first, chips.count == objects.count else {
                return false
            }
            pasteboard.declareTypes([Self.pasteboardType], owner: nil)
            return pasteboard.setString(
                chip.value.dragId,
                forType: Self.pasteboardType
            )
        }

        /// A chip dragged from a field comes back as the chip; anything
        /// else dropped is text.
        func tokenField(
            _ tokenField: NSTokenField,
            readFrom pasteboard: NSPasteboard
        ) -> [Any]? {
            guard let id = pasteboard.string(forType: Self.pasteboardType),
                let value = Piece.Value.allValues.first(where: {
                    $0.dragId == id
                })
            else { return nil }
            return [Chip(value)]
        }
    }
}
