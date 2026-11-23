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
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            MacFormField("Name") {
                BoxedField(text: $name)
            }
            MacFormField("Who sees it", note: Text(sharing.note)) {
                MacSegmentedControl(
                    selectedIndex: Binding(
                        get: { Self.choices.firstIndex(of: sharing) ?? 0 },
                        set: { sharing = Self.choices[$0] }
                    ),
                    segments: Self.choices.map {
                        .init(label: $0.label, systemImage: nil)
                    }
                )
            }
        }
    }
}
