import BaeKit
import SwiftUI

/// A receiver in a popover off its row on the Casting page, titled with its
/// name: its type, its model when it states one, and its address.
struct CastReceiverDetails: View {
    let receiver: BridgeCastReceiver

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            Text(verbatim: receiver.name)
                .themeText(.strong)
            Form {
                Section {
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
            .formStyle(.grouped)
            .scrollDisabled(true)
        }
        .padding()
        .frame(width: 320)
    }
}

#if DEBUG
    #Preview("Model stated") {
        CastReceiverDetails(
            receiver: BridgeCastReceiver(
                id: "cast-living-room",
                name: "Living Room",
                kind: .cast,
                model: "Google Nest Audio",
                address: "192.168.1.42"
            )
        )
    }

    #Preview("No model") {
        CastReceiverDetails(
            receiver: BridgeCastReceiver(
                id: "cast-den",
                name: "Den receiver",
                kind: .dlna,
                model: nil,
                address: "192.168.1.57"
            )
        )
    }
#endif
