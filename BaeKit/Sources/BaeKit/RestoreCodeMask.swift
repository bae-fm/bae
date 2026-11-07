import Foundation

/// How a restore code in groups of four shows while it is masked: the first
/// two groups (the prefix and the first four characters), the groups between
/// as bullets, and the last group, so a person can tell which code it is
/// without it being readable over their shoulder.
public enum RestoreCodeMask {
    /// What shows before the elided middle, and the last group after it.
    public struct Parts: Equatable {
        public let leading: String
        public let trailing: String

        public init(leading: String, trailing: String) {
            self.leading = leading
            self.trailing = trailing
        }
    }

    private static let bullets = "••••"

    public static func masked(_ grouped: String) -> Parts {
        let groups = grouped.split(separator: " ").map(String.init)
        guard groups.count > 3 else {
            return Parts(
                leading: String(repeating: bullets + " ", count: groups.count)
                    .trimmingCharacters(in: .whitespaces),
                trailing: ""
            )
        }
        let middle = groups.count - 3
        let leading =
            groups.prefix(2) + Array(repeating: bullets, count: middle)
        return Parts(
            leading: leading.joined(separator: " "),
            trailing: groups.last ?? ""
        )
    }
}
