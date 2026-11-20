import BaeKit
import SwiftUI

/// A receiver in a popover off its row on the Casting page, titled with its
/// name: its type, its model when it states one, and its address.
struct CastReceiverDetails: View {
    let receiver: BridgeCastReceiver

    private let popoverInset: CGFloat = 14
    /// The width of the box of rows, as the boards draw it.
    private let contentWidth: CGFloat = 320

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            Text(verbatim: receiver.name)
                .padding(.horizontal, ThemeSpace.inline)
                .themeText(.strong)
            MacBox {
                MacRow(
                    "Type",
                    value: receiver.kind.productName
                )
                if let model = receiver.model {
                    MacRow("Model", value: model)
                }
                MacRow("Address", value: receiver.address)
            }
        }
        // The popover's own inset, as the boards draw it: 14 at the sides and
        // bottom, and the title 18 down.
        .padding(.horizontal, popoverInset)
        .padding(.top, popoverInset + ThemeSpace.inline)
        .padding(.bottom, popoverInset)
        .frame(width: contentWidth + 2 * popoverInset)
    }
}
