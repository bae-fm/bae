import Foundation

extension BridgeSyncShown {
    /// The state in the person's language.
    public var statusLine: String {
        localizedCoreString(statusKey)
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

    /// When sync last finished, as "2 hours ago", while the state is a
    /// problem.
    public var lastSyncedLine: String? {
        lastSynced.map(\.agoLine)
    }
}

extension Date {
    /// How long ago this moment was, as "2 hours ago".
    public var agoLine: String {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .full
        return formatter.localizedString(for: self, relativeTo: Date())
    }
}
