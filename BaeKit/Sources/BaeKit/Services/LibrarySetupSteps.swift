/// One setup call as the app follows it: the bridge's setup, which the call
/// is handed, and its steps, for the view that draws them. Make one for each
/// call; the steps stop being read when it is released. Its stream is pulled
/// on `host`'s runtime.
public final class LibrarySetupSteps: Sendable {
    public let setup: BridgeLibrarySetup
    public let steps: LatestSource<BridgeSetupProgress>
    private let feed: LatestFeed

    public init(host: BridgeHost) {
        setup = BridgeLibrarySetup(host: host)
        let stream = LatestStream<BridgeSetupProgress>.setupProgress()
        feed = stream.feed(from: setup)
        steps = stream.only
    }
}
