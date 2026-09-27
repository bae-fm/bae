import BaeKit
import SwiftUI

/// A menu over the roles a file can be put in, showing the one in force; a
/// file already out of the tracklist gets a "Put back" button instead.
struct ImportRoleChoiceControl: View {
    let alternatives: [BridgeFileRoleChoice]
    /// The role in force, as a choice — what the menu shows selected.
    let inForce: BridgeFileRoleChoice?
    let onPick: (BridgeFileRoleChoice) -> Void

    var body: some View {
        if inForce == .notATrack {
            Button(coreString("ui.import.slots.put_back")) {
                onPick(.audio)
            }
            .buttonStyle(.link)
            .themeText(.body)
        }
        else {
            Menu {
                ForEach(alternatives, id: \.self) { choice in
                    Button {
                        onPick(choice)
                    } label: {
                        if choice == inForce {
                            Label(
                                coreString(
                                    bridgeFileRoleChoiceKey(choice: choice)
                                ),
                                systemImage: "checkmark"
                            )
                        }
                        else {
                            Text(
                                coreString(
                                    bridgeFileRoleChoiceKey(choice: choice)
                                )
                            )
                        }
                    }
                }
            } label: {
                Text(
                    inForce.map {
                        coreString(bridgeFileRoleChoiceKey(choice: $0))
                    } ?? ""
                )
                .themeText(.body)
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .foregroundStyle(.secondary)
        }
    }
}
