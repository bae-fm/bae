import AppKit
import BaeKit
import SwiftUI

/// A PIN the person reads off a receiver's screen and types: one box per
/// digit, each showing its digit as it is typed. While the field has focus,
/// the box where the next digit goes is ringed in the accent. The field takes
/// digits only, keeps leading zeros and stops at `digits`. With an `error`,
/// every box is red and the error is the line under them. Disabled, the field
/// dims and takes nothing.
struct MacPinField: View {
    /// How many digits the PIN has, as core states it.
    let digits: Int
    /// The digits typed so far.
    @Binding
    var value: String
    /// What is wrong with the PIN, under the boxes.
    var error: String?
    /// Take the keyboard as the field appears.
    var focusesOnAppear = false

    @Environment(\.isEnabled)
    private var isEnabled
    /// Whether the field's editor has the keyboard.
    @State
    private var focused = false

    /// A box's size, as the design draws it.
    private static let cellSize = CGSize(width: 40, height: 48)
    private static let digitFont = Font.system(
        size: 22,
        weight: .medium,
        design: .monospaced
    )

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            HStack(spacing: ThemeSpace.related) {
                ForEach(Array(0..<digits), id: \.self) { index in
                    cell(index)
                }
            }
            .accessibilityHidden(true)
            .overlay {
                PinEditor(
                    text: $value,
                    focused: $focused,
                    digits: digits,
                    focusesOnAppear: focusesOnAppear
                )
            }
            if let error {
                Text(verbatim: error)
                    .themeText(.detail)
                    .lineSpacing(ThemeSpace.line / 2)
                    .foregroundStyle(Theme.danger)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .opacity(isEnabled ? 1 : ThemeOpacity.off)
    }

    private func cell(_ index: Int) -> some View {
        Text(verbatim: digit(at: index))
            .font(Self.digitFont)
            .foregroundStyle(Theme.textPrimary)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .modifier(
                FieldChrome(
                    focused: ringed(index),
                    style: .boxed,
                    invalid: error != nil
                )
            )
            .frame(width: Self.cellSize.width, height: Self.cellSize.height)
    }

    private func digit(at index: Int) -> String {
        let typed = Array(value)
        return index < typed.count ? String(typed[index]) : ""
    }

    /// The box where the next digit goes, or the last box once every digit
    /// is typed, while the field has focus and the PIN is not marked wrong.
    private func ringed(_ index: Int) -> Bool {
        focused && error == nil && index == min(value.count, digits - 1)
    }
}

/// The text field a PIN is typed into: as large as the boxes it lies over and
/// not drawn, so a click on any box gives it the keyboard and the boxes show
/// what it holds. It drops anything but digits and stops at `digits`.
private struct PinEditor: NSViewRepresentable {
    @Binding
    var text: String
    @Binding
    var focused: Bool
    let digits: Int
    let focusesOnAppear: Bool

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeNSView(context: Context) -> PinEditorField {
        let coordinator = context.coordinator
        let field = PinEditorField { [coordinator] editing in
            // AppKit reports the focus while SwiftUI lays the field out.
            Task { @MainActor in
                if coordinator.parent.focused != editing {
                    coordinator.parent.focused = editing
                }
            }
        }
        field.alphaValue = 0
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.cell?.isScrollable = true
        field.cell?.wraps = false
        field.setAccessibilityLabel(String(localized: "PIN"))
        field.delegate = coordinator
        field.stringValue = text
        field.isEnabled = context.environment.isEnabled
        if focusesOnAppear, field.isEnabled {
            field.focus = coordinator.focus
            coordinator.focus.request()
        }
        return field
    }

    func updateNSView(_ field: PinEditorField, context: Context) {
        context.coordinator.parent = self
        field.isEnabled = context.environment.isEnabled
        if field.stringValue != text {
            field.stringValue = text
        }
    }

    /// Exactly the boxes' size.
    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView: PinEditorField,
        context: Context
    ) -> CGSize? {
        proposal.replacingUnspecifiedDimensions(by: .zero)
    }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: PinEditor
        let focus = FieldFocus()

        init(_ parent: PinEditor) {
            self.parent = parent
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else {
                return
            }
            let typed = field.stringValue
            let kept = String(
                typed.filter { $0.isASCII && $0.isNumber }.prefix(parent.digits)
            )
            if kept != typed { field.stringValue = kept }
            parent.text = kept
        }
    }
}

/// A text field that says when it gains and loses its field editor, which is
/// when the PIN field has the keyboard.
private final class PinEditorField: FocusingTextField {
    let onEditingChange: (Bool) -> Void

    init(onEditingChange: @escaping (Bool) -> Void) {
        self.onEditingChange = onEditingChange
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not from a nib") }

    override func becomeFirstResponder() -> Bool {
        let became = super.becomeFirstResponder()
        if became { onEditingChange(true) }
        return became
    }

    override func textDidEndEditing(_ notification: Notification) {
        super.textDidEndEditing(notification)
        onEditingChange(false)
    }
}
