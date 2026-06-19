import Foundation

extension BridgeReleaseGroup {
    /// The card's secondary line, composed for the locale: the year span across
    /// the pressings plus the pluralized pressing count, e.g. "1992 – 2012 · 4
    /// pressings". Collapses to a single year when they match, and drops the
    /// span when no pressing carries a year. bae-core owns the years and count;
    /// the UI formats them.
    var metaLabel: String {
        let countLabel = String.localizedStringWithFormat(
            NSLocalizedString(
                "core.import.pressings",
                tableName: "Core",
                bundle: .main,
                comment: ""
            ),
            pressings.count
        )
        guard let lo = yearMin, let hi = yearMax else { return countLabel }
        let span = lo == hi ? "\(lo)" : "\(lo) \u{2013} \(hi)"
        return "\(span) \u{00b7} \(countLabel)"
    }
}
