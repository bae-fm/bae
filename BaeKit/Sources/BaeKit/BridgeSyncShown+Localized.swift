import Foundation

extension BridgeSyncShown {
    /// The state in the person's language.
    public var statusLine: String {
        localizedCoreString(statusKey)
    }

    /// The state in the person's language where room is short, such as beside
    /// the Library item in the settings sidebar.
    public var shortStatusLine: String {
        localizedCoreString(shortStatusKey)
    }

    /// The color the state reads in.
    public var statusTone: StatusTone {
        switch tone {
        case .neutral: .neutral
        case .success: .success
        case .warning: .warning
        case .danger: .danger
        }
    }

    /// When sync last finished, as "2 hours ago" at `now`, while the state is
    /// a problem.
    public func lastSyncedLine(now: Date) -> String? {
        lastSynced.map { $0.agoLine(now: now) }
    }
}

extension Date {
    /// How long before `now` this moment was, as "2 hours ago" in the full
    /// style and "2 hr. ago" in the abbreviated one.
    public func agoLine(
        now: Date,
        style: RelativeDateTimeFormatter.UnitsStyle = .full
    ) -> String {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = style
        return formatter.localizedString(for: self, relativeTo: now)
    }
}
