import AppKit

/// Which of the Edit menu's clipboard actions the key window's first
/// responder can take now, asked the way AppKit asks before it enables a menu
/// item it built: the responder the action reaches, and whether that
/// responder says the item is valid.
///
/// Asked, not observed: `MenuBar` asks again each time AppKit updates its
/// windows, which it does after every event it handles, so a command reads
/// what the event before it left.
struct FirstResponderActions {
    /// The actions this answers for.
    static let clipboard: [Selector] = [
        #selector(NSText.cut(_:)),
        #selector(NSText.copy(_:)),
        #selector(NSText.paste(_:)),
        #selector(NSTextView.pasteAsPlainText(_:)),
        #selector(NSText.delete(_:)),
        #selector(NSText.selectAll(_:)),
    ]

    /// The responder an action reaches, where one does.
    private let target: @MainActor (Selector) -> Any?

    /// By default the responder chain of the running app. The menu bar is
    /// first read while the app starts, before `NSApp` exists, and until
    /// then nothing can take any of the actions.
    init(
        target: @escaping @MainActor (Selector) -> Any? = {
            NSApp?.target(forAction: $0, to: nil, from: nil)
        }
    ) {
        self.target = target
    }

    @MainActor
    func performable() -> Set<Selector> {
        Set(Self.clipboard.filter(isValid))
    }

    @MainActor
    private func isValid(_ action: Selector) -> Bool {
        guard let responder = target(action) else { return false }
        let item = NSMenuItem(title: "", action: action, keyEquivalent: "")
        if let validator = responder as? NSMenuItemValidation {
            return validator.validateMenuItem(item)
        }
        if let validator = responder as? NSUserInterfaceValidations {
            return validator.validateUserInterfaceItem(item)
        }
        return true
    }
}
