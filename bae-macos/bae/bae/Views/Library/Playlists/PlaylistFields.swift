import BaeKit
import SwiftUI

/// The fields a playlist sheet asks for: its name, and who sees it with what
/// that choice means. The New playlist and Edit playlist sheets share them.
struct PlaylistFields: View {
    @Binding
    var name: String
    @Binding
    var sharing: BridgeSharing

    private static let choices: [BridgeSharing] = [.private, .household]

    var body: some View {
        FieldColumn {
            MacTextField("Name") {
                TextField("Name", text: $name)
                    .labelsHidden()
                    .boxedTextField()
            }
            GridRow(alignment: .firstTextBaseline) {
                Text("Who sees it")
                    .gridColumnAlignment(.trailing)
                VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                    MacSegmentedControl(
                        selectedIndex: Binding(
                            get: { Self.choices.firstIndex(of: sharing) ?? 0 },
                            set: { sharing = Self.choices[$0] }
                        ),
                        segments: Self.choices.map {
                            .init(label: $0.label, systemImage: nil)
                        }
                    )
                    Text(sharing.note)
                        .fieldNote()
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}
