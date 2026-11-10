import BaeKit
import SwiftUI

/// Where an import goes, as one pop-up of three choices: this Mac, the cloud,
/// or the cloud with a whole copy kept on this Mac. The one stored choice,
/// shown beside Import on the card, on the bulk pane's Import row and in
/// Settings.
struct ImportDestinationPicker: View {
    @Binding
    var destination: BridgeImportDestination

    var body: some View {
        Picker("Import to", selection: $destination) {
            ForEach(BridgeImportDestination.choices, id: \.self) { choice in
                Text(choice.title).tag(choice)
            }
        }
        .labelsHidden()
        .pickerStyle(.menu)
        .fixedSize()
    }
}

extension BridgeImportDestination {
    /// The choices in the order the pop-up lists them.
    static var choices: [BridgeImportDestination] {
        [.thisDevice, .cloud, .cloudPinned]
    }

    fileprivate var title: LocalizedStringKey {
        switch self {
        case .thisDevice: "This Mac"
        case .cloud: "Cloud"
        case .cloudPinned: "Cloud, pinned"
        }
    }
}

#if DEBUG
    #Preview("Import destination") {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            ForEach(BridgeImportDestination.choices, id: \.self) { choice in
                ImportDestinationPicker(destination: .constant(choice))
            }
        }
        .padding()
        .windowBackground()
    }
#endif
