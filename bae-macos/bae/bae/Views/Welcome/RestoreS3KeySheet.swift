import BaeKit
import SwiftUI

/// The sheet that asks for the S3 key a restore needs when its code holds
/// none. Back returns to the code; Add starts the restore.
struct RestoreS3KeySheet: View {
    @Bindable
    var form: BootstrapCodeForm

    var body: some View {
        MacSheet(
            "S3 key",
            message: "Enter your credentials for your S3 storage.",
            width: 440
        ) {
            MacS3KeyFields(
                accessKey: $form.accessKey,
                secretKey: $form.secretKey
            ) {
                if let error = form.error {
                    MacNotice(error: error, look: .quiet)
                }
            }
        } footer: {
            MacSheetFooter {
                MacButton("Back") { form.clearReading() }
                    .keyboardShortcut(.cancelAction)
                MacButton("Add", kind: .primary) { form.start() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!form.ready)
            }
        }
    }
}
