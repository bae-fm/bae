import BaeKit
import SwiftUI

/// Confirmation for moving a local release to cloud storage. The single
/// toggle is the stored choice of whether a release that goes to the cloud
/// stays pinned on this device — the same one an import makes — written as it
/// moves; core reads it when it moves the releases.
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
        VStack(alignment: .leading, spacing: 16) {
            Text("Move to Cloud")
                .font(.headline)
            Toggle("Pinned", isOn: pinned)
            HStack {
                Spacer()
                Button("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Move to Cloud") { onConfirm() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
    }

    private var pinned: Binding<Bool> {
        Binding(
            get: { configStore.config.importStorage.pinned },
            set: { enabled in
                Task { @MainActor in
                    do { try await importer.setImportPinned(enabled) }
                    catch { uiStore.showError(error) }
                }
            }
        )
    }
}

#if DEBUG
    #Preview("Move to Cloud") {
        MoveToCloudConfirmSheet(onConfirm: {}, onCancel: {})
            .frame(width: 420)
            .environment(PreviewData.configStore())
            .environment(Importer.stub())
            .environment(UiStore())
    }
#endif
