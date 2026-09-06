import SwiftUI

private struct SourceFileEditingKey: EnvironmentKey {
    static let defaultValue = true
}

extension EnvironmentValues {
    var sourceFileEditsAllowed: Bool {
        get { self[SourceFileEditingKey.self] }
        set { self[SourceFileEditingKey.self] = newValue }
    }
}
