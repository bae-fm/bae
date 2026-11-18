import BaeKit
import SwiftUI

/// An S3 key's two fields, as every S3 key sheet asks for it: the access key
/// ID, which has the focus when the sheet opens, and the secret. `above` is
/// what the sheet says about the key it was given, such as why S3 storage
/// refused it.
struct S3KeyFields<Above: View>: View {
    @Binding
    var accessKey: String
    @Binding
    var secretKey: String
    private let above: Above

    @FocusState
    private var idFocused: Bool

    init(
        accessKey: Binding<String>,
        secretKey: Binding<String>,
        @ViewBuilder above: () -> Above = { EmptyView() }
    ) {
        _accessKey = accessKey
        _secretKey = secretKey
        self.above = above()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            above
            FieldColumn {
                MacTextField("Access key ID") {
                    TextField("Access key ID", text: $accessKey)
                        .labelsHidden()
                        .boxedTextField()
                        .focused($idFocused)
                }
                MacTextField("Secret access key") {
                    SecureField("Secret access key", text: $secretKey)
                        .labelsHidden()
                        .boxedTextField()
                }
            }
        }
        .onAppear { idFocused = accessKey.isEmpty }
    }
}
