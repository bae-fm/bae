import SwiftUI

@MainActor
public struct PauseBetweenSidesToggle: View {
    public let configStore: ConfigStore
    public let setEnabled: @Sendable (Bool) throws -> Void
    /// Takes the error, not a rendered line: whether a failure is worth showing
    /// at all is core's answer, and the sink is the one place that drops it.
    public let showError: @MainActor (any Error) -> Void

    public init(
        configStore: ConfigStore,
        setEnabled: @escaping @Sendable (Bool) throws -> Void,
        showError: @escaping @MainActor (any Error) -> Void
    ) {
        self.configStore = configStore
        self.setEnabled = setEnabled
        self.showError = showError
    }

    public var body: some View {
        Toggle("Pause between sides", isOn: binding)
    }

    private var binding: Binding<Bool> {
        Binding(
            get: { configStore.config.pauseBetweenSides },
            set: { enabled in
                do {
                    try setEnabled(enabled)
                }
                catch {
                    showError(error)
                }
            }
        )
    }
}
