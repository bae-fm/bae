import BaeKit
import SwiftUI

/// An S3 key's two fields, as every S3 key sheet asks for it: the access key
/// ID and the secret. The field the person types in next takes the focus as
/// the fields show: the ID while it is empty, else the secret. `above` is what
/// the sheet says about the key it was given, such as why S3 storage refused
/// it.
struct MacS3KeyFields<Above: View>: View {
    @Binding
    var accessKey: String
    @Binding
    var secretKey: String
    private let above: Above

    private enum Field {
        case accessKey
        case secretKey
    }

    @FocusState
    private var focused: Field?

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
            FieldColumn(labelWidth: 120) {
                MacTextField("Access key ID") {
                    BoxedField(
                        text: $accessKey,
                        focus: $focused,
                        equals: .accessKey
                    )
                }
                MacTextField("Secret access key") {
                    BoxedField(
                        text: $secretKey,
                        focus: $focused,
                        equals: .secretKey,
                        secret: true
                    )
                }
            }
        }
        .onAppear { focused = accessKey.isEmpty ? .accessKey : .secretKey }
    }
}
