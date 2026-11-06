import AppKit

/// A text field's pending request to take the keyboard: served at once when
/// the field is in a window, else as soon as it is put in one, by the
/// `FocusingTextField` it is attached to.
@MainActor
final class FieldFocus {
    private weak var field: NSTextField?
    private var pending = false

    /// Take the keyboard now, or once the field is in a window.
    func request() {
        pending = true
        serve()
    }

    fileprivate func attach(_ field: NSTextField) {
        self.field = field
        serve()
    }

    /// Take the keyboard if a request is pending and the field can.
    fileprivate func serve() {
        guard pending, let field, let window = field.window else {
            return
        }
        pending = false
        window.makeFirstResponder(field)
    }
}

/// A text field that serves its focus's request as soon as it is put in a
/// window: SwiftUI installs a representable's field before the field has
/// one.
final class FocusingTextField: NSTextField {
    var focus: FieldFocus? {
        didSet { focus?.attach(self) }
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        focus?.serve()
    }
}
