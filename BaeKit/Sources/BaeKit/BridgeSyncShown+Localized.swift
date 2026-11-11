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

extension BridgeMessage {
    /// A sync cause's message with its arguments formatted in, the time sync
    /// resumes at last when the cause names one, in the person's locale.
    public func syncCauseLine(resumesAt: Date?) -> String {
        var arguments: [CVarArg] = args.map(\.value)
        if let resumesAt {
            arguments.append(
                resumesAt.formatted(date: .omitted, time: .shortened)
            )
        }
        return String(
            format: localizedCoreString(key),
            locale: Locale.current,
            arguments: arguments
        )
    }
}
