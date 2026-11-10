import Foundation

/// What removing a library that isn't open loses, as the person confirms it.
/// It also decides how the confirmed removal runs.
public enum RemovalLoss: Sendable {
    /// The library opens, and opening it found whether this device holds
    /// changes it hasn't sent. Removing it opens it again to leave its store.
    case counted(holdsUnsent: Bool)
    /// The library can't open, so what this device hasn't sent can't be
    /// counted, and removing it, without opening it, loses all of it.
    case uncounted
}
