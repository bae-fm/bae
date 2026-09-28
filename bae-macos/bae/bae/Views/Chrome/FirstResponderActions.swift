import AppKit
import Observation

/// Which of the Edit menu's clipboard actions the key window's first
/// responder can take now, asked the way AppKit asks before it enables a menu
/// item it built: the responder the action reaches, and whether that
/// responder says the item is valid.
///
/// Asked again each time AppKit updates its windows, which it does after
/// every event it handles, so a command reads what the event before it left.
@MainActor
@Observable
final class FirstResponderActions {
    /// The actions this answers for.
    static let clipboard: [Selector] = [
        #selector(NSText.cut(_:)),
        #selector(NSText.copy(_:)),
        #selector(NSText.paste(_:)),
        #selector(NSTextView.pasteAsPlainText(_:)),
        #selector(NSText.delete(_:)),
        #selector(NSText.selectAll(_:)),
    ]

    private(set) var performable: Set<Selector> = []

    /// The responder an action reaches, where one does.
    private let target: @MainActor (Selector) -> Any?
    @ObservationIgnored
    private var updates: (any NSObjectProtocol)?

    init(
        target: @escaping @MainActor (Selector) -> Any? = {
            NSApp.target(forAction: $0, to: nil, from: nil)
        },
        notifications: NotificationCenter = .default
    ) {
        self.target = target
        updates = notifications.addObserver(
            forName: NSApplication.didUpdateNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.refresh() }
        }
        refresh()
    }

    func canPerform(_ action: Selector) -> Bool {
        performable.contains(action)
    }

    private func refresh() {
        let now = Set(Self.clipboard.filter(isValid))
        if now != performable {
            performable = now
        }
    }

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
