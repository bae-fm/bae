import AppKit

/// Edit ▸ Select All's target.
///
/// While the key window's focused list offers its own Select All
/// (`ShownRowsSelectAll`), the item runs that. Anywhere else it sends
/// `selectAll:` up the responder chain as an item with no target does, and
/// is enabled as the responder that takes it says, asked the way AppKit asks.
/// Not itself a responder, so the action it passes on never comes back to it.
@MainActor
final class SelectAllTarget: NSObject, NSMenuItemValidation {
    private static let selectAll = #selector(NSResponder.selectAll(_:))

    /// The key window's focused list's Select All, where it offers one.
    private let focusedList: @MainActor () -> ShownRowsSelectAll?
    /// The responder `selectAll:` reaches from `sender`, where one does.
    private let responder: @MainActor (_ sender: Any?) -> Any?
    /// Send `selectAll:` from `sender` to that responder; whether one took
    /// it.
    private let send: @MainActor (_ sender: Any?) -> Bool

    /// By default the key window's and the running app's responder chain.
    init(
        focusedList: @escaping @MainActor () -> ShownRowsSelectAll? = {
            let controller =
                NSApp.keyWindow?.windowController as? MainWindowController
            return controller?.shownRowsSelectAll
        },
        responder: @escaping @MainActor (_ sender: Any?) -> Any? = {
            NSApp.target(
                forAction: SelectAllTarget.selectAll,
                to: nil,
                from: $0
            )
        },
        send: @escaping @MainActor (_ sender: Any?) -> Bool = {
            NSApp.sendAction(SelectAllTarget.selectAll, to: nil, from: $0)
        }
    ) {
        self.focusedList = focusedList
        self.responder = responder
        self.send = send
    }

    @objc
    func selectAll(_ sender: Any?) {
        if let offered = focusedList()?.offered {
            offered()
            return
        }
        guard send(sender) else {
            preconditionFailure(
                "Select All is disabled with nothing to take it"
            )
        }
    }

    func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
        if focusedList()?.offered != nil {
            return true
        }
        guard let responder = responder(menuItem) else { return false }
        if let validator = responder as? NSMenuItemValidation {
            return validator.validateMenuItem(menuItem)
        }
        if let validator = responder as? NSUserInterfaceValidations {
            return validator.validateUserInterfaceItem(menuItem)
        }
        return true
    }
}
