import BaeKit
import SwiftUI

/// Confirmation for moving a local release to cloud storage, with the stored
/// choice of whether a cloud release stays pinned on this device.
struct MoveToCloudConfirmSheet: View {
    let onConfirm: () -> Void
    let onCancel: () -> Void

    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Importer.self)
    private var importer
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            Text("Move to cloud")
                .themeText(.heading)
            Toggle("Pinned", isOn: pinned)
            MacSheetFooter {
                Button("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Move to cloud") { onConfirm() }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
    }

    private var pinned: Binding<Bool> {
        Binding(
            get: { configStore.config.importDestination == .cloudPinned },
            set: { enabled in
                Task { @MainActor in
                    do {
                        try await importer.setImportDestination(
                            enabled ? .cloudPinned : .cloud
                        )
                    }
                    catch { uiStore.showError(error) }
                }
            }
        )
    }
}

#if DEBUG
    #Preview("Move to cloud") {
        MoveToCloudConfirmSheet(onConfirm: {}, onCancel: {})
            .frame(width: 420)
            .environment(PreviewData.configStore())
            .environment(Importer.stub())
            .environment(UiStore())
    }
#endif
