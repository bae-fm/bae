import SwiftUI

@MainActor
public struct PauseBetweenSidesToggle: View {
    public let configStore: ConfigStore
    public let setEnabled: @Sendable (Bool) throws -> Void
    public let showError: @MainActor (DisplayError) -> Void

    public init(
        configStore: ConfigStore,
        setEnabled: @escaping @Sendable (Bool) throws -> Void,
        showError: @escaping @MainActor (DisplayError) -> Void
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
                catch let error as BridgeError {
                    showError(DisplayError(error))
                }
                catch {
                    showError(DisplayError(error))
                }
            }
        )
    }
}
