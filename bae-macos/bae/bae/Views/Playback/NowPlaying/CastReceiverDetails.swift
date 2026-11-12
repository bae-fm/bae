import BaeKit
import SwiftUI

/// A receiver in a popover off its row on the Casting page, titled with its
/// name: its type, its model when it states one, and its address.
struct CastReceiverDetails: View {
    let receiver: BridgeCastReceiver

    private let popoverInset: CGFloat = 14

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            Text(verbatim: receiver.name)
                .padding(.horizontal, ThemeSpace.inline)
                .themeText(.strong)
            SettingsBox {
                LabeledContent(
                    "Type",
                    value: receiver.kind.productName
                )
                if let model = receiver.model {
                    LabeledContent("Model", value: model)
                }
                LabeledContent("Address", value: receiver.address)
            }
        }
        // The popover's own inset, as the boards draw it: 14 at the sides and
        // bottom, and the title 18 down.
        .padding(.horizontal, popoverInset)
        .padding(.top, popoverInset + ThemeSpace.inline)
        .padding(.bottom, popoverInset)
        .frame(width: 320)
    }
}
