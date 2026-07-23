import BaeKit

/// The fields the composer and artist browse UIs render for a summary: the
/// list row (36 pt) and the detail-pane header (72 pt) both show an image, a
/// name, and a count line.
protocol BrowseSummaryDisplay {
    var image: BridgeImageRef? { get }
    var name: String { get }
    var countText: String { get }
}

extension BridgeComposerSummary: BrowseSummaryDisplay {
    var countText: String {
        "\(workCount) \(String(localized: "Works"))"
    }
}

extension BridgeArtistSummary: BrowseSummaryDisplay {
    var countText: String {
        "\(albumCount) \(String(localized: "Albums"))"
    }
}
