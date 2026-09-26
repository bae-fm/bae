import BaeKit
import SwiftUI

/// Confirmation for moving a local release to cloud storage. The single
/// toggle chooses whether the release is also pinned on this device — the
/// same stored choice an import makes, read from and written to core's
/// preferences, so every surface that asks it starts from the last answer.
struct MoveToCloudConfirmSheet: View {
    let onConfirm: (_ pin: Bool) -> Void
    let onCancel: () -> Void

    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Importer.self)
    private var importer
    @Environment(UiStore.self)
    private var uiStore
    /// What this sheet confirms with: the stored choice it opened on, and
    /// whatever the person moves it to, which is stored as they move it.
    @State
    private var pin: Bool?

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Move to Cloud")
                .font(.headline)
            Toggle("Pinned", isOn: pinned)
                .onAppear {
                    if pin == nil {
                        pin = configStore.config.importStorage.pinned
                    }
                }
            HStack {
                Spacer()
                Button("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Move to Cloud") {
                    onConfirm(pin ?? configStore.config.importStorage.pinned)
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
    }

    private var pinned: Binding<Bool> {
        Binding(
            get: { pin ?? configStore.config.importStorage.pinned },
            set: { enabled in
                pin = enabled
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
        MoveToCloudConfirmSheet(onConfirm: { _ in }, onCancel: {})
            .frame(width: 420)
            .environment(PreviewData.configStore())
            .environment(Importer.stub())
            .environment(UiStore())
    }
#endif
