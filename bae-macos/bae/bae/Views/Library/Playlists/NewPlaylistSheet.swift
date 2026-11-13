import BaeKit
import SwiftUI

/// The New playlist sheet's content while it is open: the name typed, who
/// sees the playlist, the tracks it starts with, and the error from a failed
/// create. A new playlist starts on Only me.
struct NewPlaylistSheetState: Identifiable {
    let id = UUID()
    var name = ""
    var sharing: BridgeSharing = .private
    let trackIds: [String]
    var error: DisplayError?
}

/// Modal for making a playlist; the caller makes it and writes any error
/// back into `state`.
struct NewPlaylistSheet: View {
    @Binding
    var state: NewPlaylistSheetState
    let onCancel: () -> Void
    let onCreate: (NewPlaylistSheetState) -> Void

    private static let choices: [BridgeSharing] = [.private, .household]

    var body: some View {
        MacSheet("New playlist", width: 420) {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                FieldColumn {
                    MacTextField("Name") {
                        TextField("Name", text: $state.name)
                            .labelsHidden()
                            .boxedTextField()
                    }
                    GridRow(alignment: .firstTextBaseline) {
                        Text("Who sees it")
                            .gridColumnAlignment(.trailing)
                        VStack(
                            alignment: .leading,
                            spacing: ThemeSpace.inline
                        ) {
                            MacSegmentedControl(
                                selectedIndex: Binding(
                                    get: {
                                        Self.choices.firstIndex(
                                            of: state.sharing
                                        ) ?? 0
                                    },
                                    set: { state.sharing = Self.choices[$0] }
                                ),
                                segments: Self.choices.map {
                                    .init(label: $0.label, systemImage: nil)
                                }
                            )
                            Text(state.sharing.note)
                                .fieldNote()
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                if let error = state.error {
                    Notice(error: error)
                }
            }
        } footer: {
            MacSheetFooter {
                Button("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Create") { onCreate(state) }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(enteredNameIsBlank(text: state.name))
            }
        }
    }
}

#if DEBUG
    #Preview("New playlist") {
        @Previewable
        @State
        var state = NewPlaylistSheetState(trackIds: [])
        NewPlaylistSheet(state: $state, onCancel: {}, onCreate: { _ in })
    }

    #Preview("New playlist \u{2014} Everyone, failed") {
        @Previewable
        @State
        var state = NewPlaylistSheetState(
            name: "Late night",
            sharing: .household,
            trackIds: [],
            error: DisplayError(
                line: "Could not make the playlist",
                detail: "playlist error: the store is closed"
            )
        )
        NewPlaylistSheet(state: $state, onCancel: {}, onCreate: { _ in })
    }
#endif
