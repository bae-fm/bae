import SwiftUI

/// Confirmation for moving an unmanaged release into the library. The single
/// toggle maps to the `manage_release` pin option: whether to keep a local copy
/// (pinned for offline) once the release is managed.
struct ManageConfirmSheet: View {
    let onConfirm: (_ pin: Bool) -> Void
    let onCancel: () -> Void

    @State
    private var pin: Bool = true

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Move into library")
                .font(.headline)
            Toggle("Pin for offline", isOn: $pin)
            HStack {
                Spacer()
                Button("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Move") { onConfirm(pin) }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
    }
}

#if DEBUG
    #Preview("Move into library") {
        ManageConfirmSheet(onConfirm: { _ in }, onCancel: {})
            .frame(width: 420)
    }
#endif
