import SwiftUI

/// Confirmation for moving a local release to cloud storage. The single
/// toggle chooses whether the release is also pinned on this device.
struct MoveToCloudConfirmSheet: View {
    let onConfirm: (_ pin: Bool) -> Void
    let onCancel: () -> Void

    @State
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
