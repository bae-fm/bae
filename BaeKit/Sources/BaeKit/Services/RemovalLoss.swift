import Foundation

/// What removing a library that isn't open loses, as the person confirms it.
/// It also decides how the confirmed removal runs.
public enum RemovalLoss {
    /// The library opens: the changes this device hasn't sent, counted by
    /// opening it. Removing it opens it again to leave its store.
    case unsent(BridgeUnsentWork)
    /// The library can't open, so what this device hasn't sent can't be
    /// counted, and removing it, without opening it, loses all of it.
    case uncounted
}
