import AppKit
import BaeKit
import Combine
import SwiftUI

/// A value in a name pattern's field: one character of the field's text,
/// drawn as a chip.
final class SaveNameChip<Value: SaveNameValue>: NSTextAttachment {
    let value: Value

    init(_ value: Value, accent: NSColor) {
        self.value = value
        super.init(data: nil, ofType: nil)
        attachmentCell = MacToken(value.label, accent: accent)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }
}

/// A name pattern as one field you type in, like Finder's rename Format
/// field: values are chips among the text, Delete after a chip or Forward
/// Delete before it takes the chip out, and chips drag to where they belong.
/// Every change is stored as it is made, in order; the field holds what is
/// typed while it is being typed in, and takes the stored pattern otherwise.
///
/// The field is a text view rather than AppKit's token field, which draws its
/// tokens in the system's colour and offers no public way to draw them as
/// the chips the design system draws.
struct SaveNameField<Piece: SaveNamePiece>: NSViewRepresentable {
    typealias Chip = SaveNameChip<Piece.Value>

    /// The stored pattern.
    let pieces: [Piece]
    /// Each value the Add row sends, which the field adds once.
    let adds: AnyPublisher<Piece.Value, Never>
    /// Whether the field is the one being typed in.
    @Binding
    var focused: Bool
    let setPieces: @MainActor ([Piece]) async throws -> Void
    let fail: @MainActor (Error) -> Void

    @Environment(\.accentChoice)
    private var accent

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> SaveNameFieldView {
        let field = Self.makeField(coordinator: context.coordinator)
        context.coordinator.receive(adds)
        return field
    }

    func updateNSView(_ field: SaveNameFieldView, context: Context) {
        let coordinator = context.coordinator
        coordinator.setPieces = setPieces
        coordinator.fail = fail
        coordinator.setFocused = { focused = $0 }
        field.isEditable = context.environment.isEnabled
        field.isSelectable = context.environment.isEnabled
        coordinator.accent = accent.appKitColor
        coordinator.stored(pieces)
    }

    /// The proposed width, and the height its pieces take wrapped to it.
    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView field: SaveNameFieldView,
        context _: Context
    ) -> CGSize? {
        let width = proposal.width ?? 320
        return CGSize(width: width, height: field.height(forWidth: width))
    }

    /// A field laid out as the pane's other fields: the chrome around it is
    /// the pane's, and what it holds is the coordinator's.
    static func makeField(coordinator: Coordinator) -> SaveNameFieldView {
        let field = SaveNameFieldView()
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = ThemeText.body.nsFont
        field.textColor = NSColor(Theme.textPrimary)
        field.isRichText = true
        field.importsGraphics = false
        field.allowsUndo = true
        field.textContainerInset = .zero
        field.textContainer?.lineFragmentPadding = 0
        field.textContainer?.widthTracksTextView = true
        field.isHorizontallyResizable = false
        field.isVerticallyResizable = false
        field.typingAttributes = SaveNameFieldView.textAttributes
        field.delegate = coordinator
        field.chip = { [weak coordinator] id in coordinator?.chip(dragId: id) }
        field.dragId = { attachment in (attachment as? Chip)?.value.dragId }
        field.onFocusChange = { [weak coordinator] focused in
            coordinator?.focus(focused)
        }
        field.registerForDraggedTypes([SaveNameFieldView.chipType, .string])
        coordinator.field = field
        return field
    }

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        fileprivate weak var field: SaveNameFieldView?
        var setPieces: @MainActor ([Piece]) async throws -> Void = { _ in }
        var fail: @MainActor (Error) -> Void = { _ in }
        var setFocused: @MainActor (Bool) -> Void = { _ in }
        /// The accent the chips draw in; a change redraws them.
        var accent: NSColor = .controlAccentColor {
            didSet {
                guard accent != oldValue, let field else { return }
                let pieces = pieces(in: field)
                field.storage.setAttributedString(text(for: pieces))
            }
        }

        /// The Add row's values, as the field receives them.
        private var adds: AnyCancellable?

        /// The pattern as last stored.
        private var storedPieces: [Piece] = []
        /// The pattern of the write in flight.
        private var handed: [Piece]?
        /// The newest pattern waiting for the write in flight.
        private var queued: [Piece]?
        /// The writes in flight and queued, until the last returns. While
        /// they run, the field draws what it handed over; once they return,
        /// it draws the stored pattern.
        private(set) var writes: Task<Void, Never>?

        func focus(_ focused: Bool) {
            setFocused(focused)
            if !focused {
                changed()
                settle()
            }
        }

        /// The store shows `pieces`. A field not being typed in takes them.
        func stored(_ pieces: [Piece]) {
            storedPieces = pieces
            settle()
        }

        /// A field not being typed in and with no write in flight draws the
        /// stored pattern.
        private func settle() {
            guard let field, !field.isEditing, writes == nil else { return }
            guard pieces(in: field) != storedPieces else { return }
            field.storage.setAttributedString(text(for: storedPieces))
        }

        /// The pattern the field holds now: each chip a value, each run of
        /// text between them typed text.
        func pieces(in field: SaveNameFieldView) -> [Piece] {
            let storage = field.storage
            var pieces: [Piece] = []
            var text = ""
            func endText() {
                if !text.isEmpty {
                    pieces.append(Piece(text: text))
                    text = ""
                }
            }
            let whole = NSRange(location: 0, length: storage.length)
            storage.enumerateAttribute(.attachment, in: whole) {
                attachment,
                range,
                _ in
                switch attachment {
                case let chip as Chip:
                    endText()
                    pieces.append(Piece(value: chip.value))
                case nil:
                    text += (storage.string as NSString).substring(with: range)
                default:
                    preconditionFailure(
                        "a name pattern's field holds only chips and text"
                    )
                }
            }
            endText()
            return pieces
        }

        /// `pieces` as the field's text: each value a chip, each typed text
        /// as typed.
        func text(for pieces: [Piece]) -> NSAttributedString {
            let text = NSMutableAttributedString()
            for piece in pieces {
                switch piece.content {
                case .value(let value):
                    text.append(
                        NSAttributedString(
                            attachment: Chip(value, accent: accent)
                        )
                    )
                case .text(let typed):
                    text.append(
                        NSAttributedString(
                            string: typed,
                            attributes: SaveNameFieldView.textAttributes
                        )
                    )
                }
            }
            return text
        }

        /// The chip of the value whose drag id is `id`.
        func chip(dragId id: String) -> NSTextAttachment? {
            Piece.Value.allValues.first { $0.dragId == id }
                .map { Chip($0, accent: accent) }
        }

        /// Add each value the Add row sends. The row sends from its button's
        /// action, on the main actor.
        func receive(_ adds: AnyPublisher<Piece.Value, Never>) {
            self.adds = adds.sink { [weak self] value in
                MainActor.assumeIsolated { self?.append(value) }
            }
        }

        /// Put `value` as a chip at the end, after what is typed.
        func append(_ value: Piece.Value) {
            guard let field else { return }
            // Ends the typing, which hands what is typed to the store.
            field.window?.makeFirstResponder(nil)
            let pieces = pieces(in: field) + [Piece(value: value)]
            field.storage.setAttributedString(text(for: pieces))
            submit(pieces)
        }

        /// The field's pattern changed: typed into, a chip removed or moved.
        private func changed() {
            guard let field else { return }
            submit(pieces(in: field))
        }

        func textDidChange(_: Notification) {
            changed()
        }

        /// Store `pieces` after the writes before it, one at a time, so the
        /// store ends with the last one.
        private func submit(_ pieces: [Piece]) {
            guard pieces != (queued ?? handed ?? storedPieces) else { return }
            queued = pieces
            guard writes == nil else { return }
            writes = Task {
                await drain()
            }
        }

        private func drain() async {
            while let next = queued {
                queued = nil
                handed = next
                do {
                    try await setPieces(next)
                }
                catch {
                    // The store keeps the pattern it had.
                    fail(error)
                }
            }
            handed = nil
            writes = nil
            settle()
        }
    }
}

/// The text view the field is: one paragraph that wraps, which Return
/// and Tab leave rather than type into, and which drags and pastes a
/// chip as that chip and anything else as plain text.
final class SaveNameFieldView: NSTextView {
    var onFocusChange: ((Bool) -> Void)?
    /// The chip a drag carries, by its value's drag id.
    var chip: (String) -> NSTextAttachment? = { _ in nil }
    /// The drag id of a chip's value.
    var dragId: (NSTextAttachment) -> String? = { _ in nil }

    static let chipType = NSPasteboard.PasteboardType(
        "fm.bae.save-name-chip"
    )

    /// How typed text looks in the field.
    static var textAttributes: [NSAttributedString.Key: Any] {
        [
            .font: ThemeText.body.nsFont,
            .foregroundColor: NSColor(Theme.textPrimary),
        ]
    }

    /// The field's text, which a text view built with its own text system
    /// always holds.
    var storage: NSTextStorage {
        guard let textStorage else {
            preconditionFailure(
                "a text view built whole holds its text storage"
            )
        }
        return textStorage
    }

    /// Whether the field is the one being typed in.
    var isEditing: Bool {
        window?.firstResponder === self
    }

    override func becomeFirstResponder() -> Bool {
        let became = super.becomeFirstResponder()
        if became {
            onFocusChange?(true)
        }
        return became
    }

    override func resignFirstResponder() -> Bool {
        let resigned = super.resignFirstResponder()
        if resigned {
            onFocusChange?(false)
        }
        return resigned
    }

    /// The height the field's text takes wrapped to `width`.
    func height(forWidth width: CGFloat) -> CGFloat {
        guard let container = textContainer, let layout = layoutManager else {
            preconditionFailure(
                "a text view built whole holds its container and layout"
            )
        }
        let old = container.size
        container.size = NSSize(width: width, height: .greatestFiniteMagnitude)
        layout.ensureLayout(for: container)
        let used = layout.usedRect(for: container).height
        container.size = old
        let line = ceil(
            layout.defaultLineHeight(for: font ?? ThemeText.body.nsFont)
        )
        return ceil(max(used, line))
    }

    /// The text in `range` as the person sees it: each chip by its value's
    /// name, where the text holds one attachment character for it.
    func readableText(in range: NSRange) -> String {
        let storage = self.storage
        var text = ""
        storage.enumerateAttribute(.attachment, in: range) {
            attachment,
            run,
            _ in
            if let cell = (attachment as? NSTextAttachment)?.attachmentCell
                as? NSCell,
                let label = cell.accessibilityLabel()
            {
                text += label
            }
            else {
                text += (storage.string as NSString).substring(with: run)
            }
        }
        return text
    }

    /// VoiceOver reads the field as the person sees it, each chip by its
    /// value's name among the typed text.
    override func accessibilityValue() -> String? {
        readableText(in: NSRange(location: 0, length: storage.length))
    }

    override func accessibilityString(for range: NSRange) -> String? {
        readableText(in: range)
    }

    /// A name is one line of text: Return ends the typing, as it does in
    /// a text field.
    override func insertNewline(_: Any?) {
        window?.makeFirstResponder(nil)
    }

    override func insertTab(_: Any?) {
        window?.selectNextKeyView(self)
    }

    override func insertBacktab(_: Any?) {
        window?.selectPreviousKeyView(self)
    }

    /// The chip the selection is, when it is exactly one chip.
    private var selectedChip: NSTextAttachment? {
        let range = selectedRange()
        guard range.length == 1,
            range.location < storage.length
        else { return nil }
        return storage.attribute(
            .attachment,
            at: range.location,
            effectiveRange: nil
        ) as? NSTextAttachment
    }

    override var writablePasteboardTypes: [NSPasteboard.PasteboardType] {
        selectedChip == nil ? [.string] : [Self.chipType]
    }

    override func writeSelection(
        to pasteboard: NSPasteboard,
        type: NSPasteboard.PasteboardType
    ) -> Bool {
        if type == Self.chipType {
            guard let chip = selectedChip, let id = dragId(chip) else {
                return false
            }
            return pasteboard.setString(id, forType: Self.chipType)
        }
        return super.writeSelection(to: pasteboard, type: type)
    }

    override var readablePasteboardTypes: [NSPasteboard.PasteboardType] {
        [Self.chipType, .string]
    }

    override var acceptableDragTypes: [NSPasteboard.PasteboardType] {
        [Self.chipType, .string]
    }

    /// A chip comes back as the chip; any text comes in as plain text
    /// on one line.
    override func readSelection(
        from pasteboard: NSPasteboard,
        type: NSPasteboard.PasteboardType
    ) -> Bool {
        let replacement: NSAttributedString
        if type == Self.chipType {
            guard let id = pasteboard.string(forType: Self.chipType),
                let chip = chip(id)
            else { return false }
            replacement = NSAttributedString(attachment: chip)
        }
        else if let text = pasteboard.string(forType: .string) {
            replacement = NSAttributedString(
                string: text.components(separatedBy: .newlines)
                    .joined(separator: " "),
                attributes: SaveNameFieldView.textAttributes
            )
        }
        else {
            return false
        }
        let range = rangeForUserTextChange
        guard range.location != NSNotFound,
            shouldChangeText(in: range, replacementString: replacement.string)
        else { return false }
        storage.replaceCharacters(in: range, with: replacement)
        didChangeText()
        return true
    }
}
