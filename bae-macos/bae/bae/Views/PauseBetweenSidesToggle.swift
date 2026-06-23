import SwiftUI

@MainActor
struct PauseBetweenSidesToggle: View {
    let configStore: ConfigStore
    let appHandle: AppHandle
    let showError: @MainActor (DisplayError) -> Void

    var body: some View {
        Toggle("Pause between sides", isOn: binding)
    }

    private var binding: Binding<Bool> {
        Binding(
            get: { configStore.config.pauseBetweenSides },
            set: { enabled in
                do {
                    try appHandle.setPauseBetweenSides(enabled: enabled)
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
