/// One setup call as the app follows it: the bridge's setup, which the call
/// is handed, and its steps, for the view that draws them. Make one for each
/// call; the steps stop being read when it is released. A throw while the
/// steps are read is reported to `diagnostics`, the process's sink.
public final class LibrarySetupSteps: Sendable {
    public let setup: BridgeLibrarySetup
    public let steps: LatestSource<BridgeSetupProgress>
    private let feed: LatestFeed

    public init(diagnostics: BridgeDiagnostics) {
        setup = BridgeLibrarySetup(diagnostics: diagnostics)
        let stream = LatestStream<BridgeSetupProgress>.setupProgress()
        feed = stream.feed(from: setup)
        steps = stream.only
    }
}
