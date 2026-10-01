import Observation

/// A command a focused view offers the menu bar through its focused values:
/// a menu item sends it, and the view that offered it carries it out when
/// it sees the send.
///
/// One object for the offering view's lifetime, kept in its `@State`, rather
/// than a closure: SwiftUI rebuilds the main menu's menus whenever the key
/// window's focused values change, and AppKit throws when that happens to a
/// menu that is open. A closure published from a view's body is a new value
/// on every render of that view, so a render while a menu was open — search
/// results landing, an import moving a row — changed the focused values and
/// crashed the app.
@MainActor
@Observable
final class FocusedCommand {
    /// How many times the command has been sent; the offering view acts on
    /// each change.
    private(set) var sends = 0

    func send() {
        sends += 1
    }
}
