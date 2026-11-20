import AppKit
import BaeKit
import SwiftUI

/// A value in a name pattern's field: one character of the field's text,
/// drawn as a chip.
final class SaveNameChip<Value: SaveNameValue>: NSTextAttachment {
    let value: Value

    init(_ value: Value, accent: NSColor) {
        self.value = value
        super.init(data: nil, ofType: nil)
        attachmentCell = SaveNameChipCell(label: value.label, accent: accent)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }
}

/// How a chip draws, as the design system's MacToken draws it: the value's
/// name in the accent, on the accent's tint, with the chip's small corners
/// and no ×. VoiceOver reads the chip by the value's name.
final class SaveNameChipCell: NSTextAttachmentCell {
    private let label: String
    private let accent: NSColor

    /// The space inside the chip at each side of the name.
    private static let padding: CGFloat = 8
    /// The space outside the chip at each side, between it and the text.
    private static let margin: CGFloat = 2
    /// The chip's height: the body text's line.
    private static let height: CGFloat = 16

    private static var font: NSFont { ThemeText.body.nsFont }

    init(label: String, accent: NSColor) {
        self.label = label
        self.accent = accent
        super.init(textCell: "")
    }

    @available(*, unavailable)
    required init(coder _: NSCoder) {
        fatalError()
    }

    private var attributedLabel: NSAttributedString {
        NSAttributedString(
            string: label,
            attributes: [.font: Self.font, .foregroundColor: accent]
        )
    }

    override func cellSize() -> NSSize {
        NSSize(
            width: ceil(attributedLabel.size().width) + 2 * Self.padding
                + 2 * Self.margin,
            height: Self.height
        )
    }

    /// The chip sits on the text's line: its bottom is the font's descender
    /// below the baseline.
    override func cellBaselineOffset() -> NSPoint {
        NSPoint(x: 0, y: Self.font.descender - (Self.height - lineHeight) / 2)
    }

    private var lineHeight: CGFloat {
        Self.font.ascender - Self.font.descender
    }

    override func draw(withFrame cellFrame: NSRect, in _: NSView?) {
        let chip = cellFrame.insetBy(dx: Self.margin, dy: 0)
        let tint = accent.withAlphaComponent(ThemeOpacity.tint)
        tint.setFill()
        NSBezierPath(
            roundedRect: chip,
            xRadius: ThemeRadius.chip,
            yRadius: ThemeRadius.chip
        )
        .fill()
        let text = attributedLabel
        let size = text.size()
        text.draw(
            at: NSPoint(
                x: chip.midX - size.width / 2,
                y: chip.midY - size.height / 2
            )
        )
    }

    override func draw(
        withFrame cellFrame: NSRect,
        in controlView: NSView?,
        characterIndex _: Int,
        layoutManager _: NSLayoutManager
    ) {
        draw(withFrame: cellFrame, in: controlView)
    }

    override func accessibilityLabel() -> String? {
        label
    }
}

/// How the pane's Add row reaches the field it adds a chip to.
@MainActor
final class MacFileNameFieldHandle<Piece: SaveNamePiece> {
    fileprivate weak var field: SaveNameField<Piece>.Coordinator?

    func append(_ value: Piece.Value) {
        field?.append(value)
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
    let handle: MacFileNameFieldHandle<Piece>
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
        handle.field = context.coordinator
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
        field.textColor = .labelColor
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
                field.textStorage?.setAttributedString(text(for: pieces))
            }
        }

        /// The pattern as last stored.
        private var storedPieces: [Piece] = []
        /// The last pattern handed to the store, until the store shows it:
        /// a field that shows it already isn't reset to the older pattern
        /// the store still shows.
        private var lastSubmitted: [Piece]?
        /// The newest pattern waiting for the write in flight.
        private var queued: [Piece]?
        /// Whether a write is in flight or queued.
        private(set) var writing = false

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
            if pieces == lastSubmitted {
                lastSubmitted = nil
            }
            settle()
        }

        private func settle() {
            guard let field, !field.isEditing else { return }
            let shown = pieces(in: field)
            guard shown != storedPieces, shown != lastSubmitted else { return }
            field.textStorage?.setAttributedString(text(for: storedPieces))
        }

        /// The pattern the field holds now: each chip a value, each run of
        /// text between them typed text.
        func pieces(in field: NSTextView) -> [Piece] {
            guard let storage = field.textStorage else { return [] }
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
                if let chip = attachment as? Chip {
                    endText()
                    pieces.append(Piece(value: chip.value))
                }
                else {
                    // A character the text holds for an attachment that is
                    // no chip is no text the person typed.
                    text += (storage.string as NSString).substring(with: range)
                        .replacingOccurrences(of: "\u{FFFC}", with: "")
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

        /// Put `value` as a chip at the end, after what is typed.
        func append(_ value: Piece.Value) {
            guard let field else { return }
            // Ends the typing, which hands what is typed to the store.
            field.window?.makeFirstResponder(nil)
            let pieces = pieces(in: field) + [Piece(value: value)]
            field.textStorage?.setAttributedString(text(for: pieces))
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
        [.font: ThemeText.body.nsFont, .foregroundColor: NSColor.labelColor]
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
        guard let container = textContainer,
            let layout = layoutManager
        else { return 0 }
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
        guard range.length == 1, let storage = textStorage,
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
        textStorage?.replaceCharacters(in: range, with: replacement)
        didChangeText()
        return true
    }
}
