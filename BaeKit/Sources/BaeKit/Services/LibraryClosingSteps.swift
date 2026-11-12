/// One close of a library as the app follows it: the bridge's closing,
/// which the close is handed, and its steps, for the view that draws them.
/// Make one for each close the app shows; the steps stop being read when it
/// is released. Its stream is pulled on `host`'s runtime, which runs on after
/// the library's own has ended.
public final class LibraryClosingSteps: Sendable {
    public let closing: BridgeLibraryClosing
    public let steps: LatestSource<BridgeClosingProgress>
    /// The feed from the closing, nil for a shot's, whose stream holds a
    /// step and nothing feeds it.
    private let feed: LatestFeed?

    public init(host: BridgeHost) {
        closing = BridgeLibraryClosing(host: host)
        let stream = LatestStream<BridgeClosingProgress>.closingSteps()
        feed = stream.feed(from: closing)
        steps = stream.only
    }

    #if DEBUG
        /// A close a shot shows at `step`: its stream holds the
        /// step, and the closing it is handed never runs.
        public init(host: BridgeHost, at step: BridgeClosingProgress) {
            closing = BridgeLibraryClosing(host: host)
            feed = nil
            steps = LatestStream.closingSteps(holding: step).only
        }
    #endif
}
