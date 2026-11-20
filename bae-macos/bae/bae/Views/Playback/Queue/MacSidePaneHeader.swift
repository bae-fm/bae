import BaeKit
import SwiftUI

/// What starts the side pane's top: nothing, the + that makes a new playlist
/// on the Playlists tab, or, while a playlist is open there, the back button
/// to the list.
enum SidePaneLead {
    case none
    case newPlaylist(() -> Void)
    case back(to: String, () -> Void)
}

/// The side pane's head: the tabs centred, Close at the end, and `lead` at the
/// start.
struct MacSidePaneHeader: View {
    let tab: SidePaneTab
    let onSelectTab: (SidePaneTab) -> Void
    let lead: SidePaneLead
    let onClose: () -> Void

    /// Where `tab` sits among the control's segments, which list every tab.
    private static func index(of tab: SidePaneTab) -> Int {
        guard let index = SidePaneTab.allCases.firstIndex(of: tab) else {
            preconditionFailure("the side pane's control lists every tab")
        }
        return index
    }

    var body: some View {
        ZStack {
            MacSegmentedControl(
                selectedIndex: Binding(
                    get: { Self.index(of: tab) },
                    set: { onSelectTab(SidePaneTab.allCases[$0]) }
                ),
                segments: SidePaneTab.allCases.map {
                    MacSegmentedControl.Segment(
                        label: $0.title,
                        systemImage: nil
                    )
                }
            )
            HStack(spacing: 0) {
                leadControl
                Spacer(minLength: 0)
                MacCloseButton(onClose: onClose)
            }
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.top, ThemeSpace.edge)
        .padding(.bottom, ThemeSpace.group)
    }

    @ViewBuilder
    private var leadControl: some View {
        switch lead {
        case .none:
            EmptyView()
        case .newPlaylist(let action):
            MacIconButton(
                symbol: "plus",
                label: "New playlist…",
                action: action
            )
        case .back(let to, let action):
            MacBackButton(backTitle: to, onBack: action)
        }
    }
}
