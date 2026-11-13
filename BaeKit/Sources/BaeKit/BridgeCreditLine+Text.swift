import Foundation

extension BridgeCreditLine {
    /// The line in the viewer's language. Each run of names with no join
    /// between them is joined by the list formatter; a recognized join is
    /// worded from its `Core` message, and any other join is shown as its
    /// source wrote it.
    public var text: String {
        var runs: [(names: [String], join: BridgeCreditJoin?)] = []
        var run: [String] = []
        for credited in names {
            run.append(credited.name)
            if let join = credited.join {
                runs.append((run, join))
                run = []
            }
        }
        if !run.isEmpty || runs.isEmpty {
            runs.append((run, nil))
        }
        var text = ListFormatter.localizedString(byJoining: runs[0].names)
        for index in 1..<runs.count {
            let after = ListFormatter.localizedString(
                byJoining: runs[index].names
            )
            text = Self.joined(text, runs[index - 1].join, after)
        }
        return text
    }

    private static func joined(
        _ before: String,
        _ join: BridgeCreditJoin?,
        _ after: String
    ) -> String {
        switch join {
        case nil:
            return before + after
        case .unrecognized(let text):
            return before + text + after
        case let recognized?:
            guard let key = bridgeCreditJoinKey(join: recognized) else {
                preconditionFailure(
                    "a recognized credit join has a catalog message"
                )
            }
            return localizedCoreString(key, before, after)
        }
    }
}

extension BridgeCreditLine {
    /// A line of one name, for a fixture or a preview.
    public static func named(_ name: String) -> BridgeCreditLine {
        BridgeCreditLine(names: [BridgeCreditedName(name: name, join: nil)])
    }
}
