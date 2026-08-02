import BaeKit
import SwiftUI

/// The mapping table's control for a file whose job is a decision: a menu over
/// the roles it can be put in, showing the one in force.
///
/// A file already out of the tracklist gets the shorthand instead — one "Put
/// back" button, because the only thing left to say about it is that it was a
/// track after all.
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
            .font(.system(size: 12))
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
                .font(.system(size: 12))
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .foregroundStyle(.secondary)
        }
    }
}
