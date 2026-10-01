import AppKit
import Observation
import os.log

private let firstResponderLogger = Logger.bae("FirstResponderActions")

/// Which of the Edit menu's clipboard actions the key window's first
/// responder can take now, asked the way AppKit asks before it enables a menu
/// item it built: the responder the action reaches, and whether that
/// responder says the item is valid.
///
/// Asked each time AppKit updates its windows, which it does after every
/// event it handles, so a command reads what the event before it left. Never
/// asked before the first update: this is made while the app starts, before
/// `NSApp` exists, and until then nothing can take any of the actions.
///
/// Not asked while a menu is open, and asked again once it closes: a change
/// here makes SwiftUI replace the open menu's items, and AppKit throws when
/// a menu on screen has its items replaced. AppKit's own items are likewise
/// validated as their menu opens and held while it is open.
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

    /// How many menus are being tracked now; nothing is asked while any is.
    @ObservationIgnored
    private var openMenus = 0

    init(
        target: @escaping @MainActor (Selector) -> Any? = {
            NSApp.target(forAction: $0, to: nil, from: nil)
        },
        notifications: NotificationCenter = .default
    ) {
        self.target = target
        // Never removed: the app holds one of these for its whole life.
        _ = notifications.addObserver(
            forName: NSApplication.didUpdateNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.refresh() }
        }
        _ = notifications.addObserver(
            forName: NSMenu.didBeginTrackingNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.openMenus += 1 }
        }
        _ = notifications.addObserver(
            forName: NSMenu.didEndTrackingNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                if self.openMenus == 0 {
                    firstResponderLogger.warning(
                        "A menu ended tracking that never began; asking again"
                    )
                }
                else {
                    self.openMenus -= 1
                }
                self.refresh()
            }
        }
    }

    func canPerform(_ action: Selector) -> Bool {
        performable.contains(action)
    }

    private func refresh() {
        guard openMenus == 0 else { return }
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
