import SwiftUI

@MainActor
public struct PauseBetweenSidesToggle: View {
    private let configStore: ConfigStore
    private let setEnabled: @Sendable (Bool) async throws -> Void
    /// Takes the error, not a rendered line: whether a failure is worth showing
    /// at all is core's answer, and the sink is the one place that drops it.
    private let showError: @MainActor (any Error) -> Void

    public init(
        configStore: ConfigStore,
        setEnabled: @escaping @Sendable (Bool) async throws -> Void,
        showError: @escaping @MainActor (any Error) -> Void
    ) {
        self.configStore = configStore
        self.setEnabled = setEnabled
        self.showError = showError
    }

    public var body: some View {
        Toggle("Pause between sides and discs", isOn: binding)
    }

    private var binding: Binding<Bool> {
        Binding(
            get: { configStore.config.pauseBetweenSides },
            set: { enabled in
                // The write is a durable file replace, awaited off the main
                // thread; the config mirror re-renders once it lands.
                Task {
                    do {
                        try await setEnabled(enabled)
                    }
                    catch {
                        showError(error)
                    }
                }
            }
        )
    }
}
