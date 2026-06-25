import SwiftUI

@MainActor
struct PauseBetweenSidesToggle: View {
    let configStore: ConfigStore
    let setEnabled: @Sendable (Bool) throws -> Void
    let showError: @MainActor (DisplayError) -> Void

    var body: some View {
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
                    showError(DisplayError(line: error.localizedDescription))
                }
            }
        )
    }
}
