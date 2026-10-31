/// One open of a library as the app follows it: the bridge's opening, which
/// the call that opens the library is handed, and its steps, for the view
/// that draws them. Make one for each call; the steps stop being read when
/// it is released. A throw while the steps are read is reported to
/// `diagnostics`, the process's sink.
public final class LibraryOpenProgress: Sendable {
    public let opening: BridgeLibraryOpening
    public let steps: LatestSource<BridgeOpenProgress>
    private let feed: LatestFeed

    public init(diagnostics: BridgeDiagnostics) {
        opening = BridgeLibraryOpening(diagnostics: diagnostics)
        let stream = LatestStream<BridgeOpenProgress>.openProgress()
        feed = stream.feed(from: opening)
        steps = stream.only
    }
}
