import Foundation

extension BridgeCreditRole {
    /// The role in the viewer's language: its word, then its qualifiers in
    /// parentheses joined by the list formatter ("Engineer (assistant)").
    public var text: String {
        guard !qualifiers.isEmpty else {
            return role.text
        }
        return localizedCoreString(
            "core.credit.qualified",
            role.text,
            ListFormatter.localizedString(byJoining: qualifiers.map(\.text))
        )
    }
}

extension BridgeCreditRole {
    /// A role of one word, for a fixture or a preview.
    public static func word(_ text: String) -> BridgeCreditRole {
        BridgeCreditRole(role: .verbatim(text: text), qualifiers: [])
    }
}
