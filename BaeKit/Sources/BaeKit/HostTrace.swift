import Foundation

/// TRACE(import-list-diagnosis): host-side trace lines, written into the same
/// rolling file log under `~/.bae/logs/` that the core writes.
///
/// Not `BaeLogger`: that writes to unified logging only, which keeps `info` and
/// `debug` in memory and shows them to `log show` only when asked for them by
/// name — which is how the first attempt at this trace left the entire Swift
/// half of the import list's read invisible while the core half was in the file
/// all along. One file, one stream, both halves in order.
///
/// Goes out with the rest of the trace.
public enum HostTrace {
    public static func line(_ category: String, _ message: String) {
        bridgeHostTrace(line: "\(category): \(message)")
    }
}
