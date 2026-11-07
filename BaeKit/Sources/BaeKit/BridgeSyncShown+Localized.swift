import Foundation

extension BridgeSyncShown {
    /// The state in the person's language.
    public var statusLine: String {
        localizedCoreString(bridgeSyncStatusKey(status: status))
    }

    /// Why the state is what it is, in the person's language.
    public var causeLine: String? {
        cause?.localizedLine
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
        lastSynced.map { moment in
            let formatter = RelativeDateTimeFormatter()
            formatter.unitsStyle = .full
            return formatter.localizedString(for: moment, relativeTo: Date())
        }
    }
}

extension BridgeSyncCause {
    /// The cause's message with its arguments formatted in, a time sync
    /// resumes at last, in the person's locale.
    public var localizedLine: String {
        var arguments: [CVarArg] = message.args.map(\.value)
        if let resumesAt {
            arguments.append(
                resumesAt.formatted(date: .omitted, time: .shortened)
            )
        }
        return String(
            format: localizedCoreString(message.key),
            locale: Locale.current,
            arguments: arguments
        )
    }
}
