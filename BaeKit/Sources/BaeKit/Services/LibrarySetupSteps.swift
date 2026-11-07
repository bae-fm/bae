/// What a setup call is doing now: a step of its own, or a step coven runs
/// as the call sets the library's storage up.
public enum SetupLine: Sendable, Equatable {
    case step(BridgeSetupProgress)
    case storage(BridgeStorageSetupProgress)
}

/// One setup call as the app follows it: the bridge's setup, which the call
/// is handed, and its steps, for the view that draws them. Make one for each
/// call; the steps stop being read when it is released. Its stream is pulled
/// on `host`'s runtime.
public final class LibrarySetupSteps: Sendable {
    public let setup: BridgeLibrarySetup
    public let steps: LatestSource<SetupLine>
    private let feed: LatestFeed

    public init(host: BridgeHost) {
        setup = BridgeLibrarySetup(host: host)
        let stream = LatestStream<SetupLine>.setupLines()
        feed = stream.feed(from: setup)
        steps = stream.only
    }
}
