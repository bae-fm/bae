import SwiftUI

/// The small capitalised label that names a section, a group or a column.
public struct MacEyebrow: View {
    private let text: Text

    public init(_ text: Text) {
        self.text = text
    }

    public init(_ key: LocalizedStringKey) {
        text = Text(key)
    }

    public init(verbatim string: String) {
        text = Text(verbatim: string)
    }

    public var body: some View {
        text
            .themeText(.eyebrow)
            .foregroundStyle(Theme.macTextSecondary)
    }
}
