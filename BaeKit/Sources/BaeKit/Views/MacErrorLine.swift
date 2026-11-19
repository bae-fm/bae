import SwiftUI

/// A line saying something failed.
public struct MacErrorLine: View {
    private let text: Text

    /// A message already in the person's language; a literal takes the
    /// localized initializer instead.
    @_disfavoredOverload
    public init(_ message: String) {
        text = Text(verbatim: message)
    }

    public init(_ key: LocalizedStringKey) {
        text = Text(key)
    }

    public var body: some View {
        text
            .themeText(.body)
            .foregroundStyle(Theme.danger)
    }
}
