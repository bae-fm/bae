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
            message: "Enter your credentials for the S3 bucket.",
            width: 440
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                Grid(
                    alignment: .leading,
                    horizontalSpacing: ThemeSpace.group,
                    verticalSpacing: ThemeSpace.related
                ) {
                    GridRow {
                        Text("Access key ID")
                            .gridColumnAlignment(.trailing)
                        TextField("", text: $form.accessKey)
                            .accessibilityLabel("Access key ID")
                            .boxedTextField()
                    }
                    GridRow {
                        Text("Secret access key")
                        SecureField("", text: $form.secretKey)
                            .accessibilityLabel("Secret access key")
                            .boxedTextField()
                    }
                }
                if let error = form.error {
                    Notice(error: error)
                }
            }
        } footer: {
            MacSheetFooter {
                Button("Back") { form.clearReading() }
                    .keyboardShortcut(.cancelAction)
                Button("Add") { form.start() }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(!form.ready)
            }
        }
    }
}

#if DEBUG
    #Preview("S3 key") {
        RestoreS3KeySheet(
            form: LibrarySetup.stub()
                .bootstrapForm(entry: .code, onLibraryReady: { _, _ in })
        )
        .windowBackground()
    }
#endif
