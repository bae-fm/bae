import BaeKit
import SwiftUI

/// Where an import goes, as one pop-up of two choices: this Mac or the cloud.
/// The one stored choice, shown beside Import on the card, on the bulk pane's
/// Import row and in Settings.
struct MacImportDestination: View {
    @Binding
    var destination: BridgeImportDestination

    var body: some View {
        MacPopUpButton("Import to", selection: $destination) {
            for choice in BridgeImportDestination.choices {
                MacPopUpOption(choice.title, value: choice)
            }
        }
        .fixedSize()
    }
}

extension BridgeImportDestination {
    /// The choices in the order the pop-up lists them.
    static var choices: [BridgeImportDestination] {
        [.thisDevice, .cloud]
    }

    fileprivate var title: LocalizedStringResource {
        switch self {
        case .thisDevice: "This Mac"
        case .cloud: "Cloud"
        }
    }
}

#if DEBUG
    #Preview("Import destination") {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            ForEach(BridgeImportDestination.choices, id: \.self) { choice in
                MacImportDestination(destination: .constant(choice))
            }
        }
        .padding()
        .windowBackground()
    }
#endif
