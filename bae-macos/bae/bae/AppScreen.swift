/// What the main window shows: a library opening, the welcome, a library's
/// recovery after it couldn't open or sync, or the open library.
enum AppScreen {
    case loading
    case welcome
    /// Opening found a damaged database or another copy of this device; the
    /// person can reset the device for this library.
    case resetRequired(libraryId: String)
    /// Another of the person's devices replaced their keys; the library is
    /// closed and the person restores it with their current restore code.
    case restoreRequired(libraryId: String)
    case keychainLocked(libraryId: String)
    case library
}
