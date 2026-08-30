import SwiftUI

enum StoragePinPreference {
    /// Shared by every UI that asks whether a cloud-stored release should also
    /// stay downloaded on this device. The stored key preserves the choice
    /// already written by Import.
    static let userDefaultsKey = "importStoragePinned"
}

/// Confirmation for moving a local release to cloud storage. The single
/// toggle chooses whether the release is also pinned on this device.
struct MoveToCloudConfirmSheet: View {
    let onConfirm: (_ pin: Bool) -> Void
    let onCancel: () -> Void

    @AppStorage(StoragePinPreference.userDefaultsKey)
    private var pin: Bool = true

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Move to Cloud")
                .font(.headline)
            Toggle("Pinned", isOn: $pin)
            HStack {
                Spacer()
                Button("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Move to Cloud") { onConfirm(pin) }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
    }
}

#if DEBUG
    #Preview("Move to Cloud") {
        MoveToCloudConfirmSheet(onConfirm: { _ in }, onCancel: {})
            .frame(width: 420)
    }
#endif
