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

/// The side pane docked at the window's right. Its top holds the tabs, centred,
/// and Close at its end, with `lead` at its start; the tab's content fills the
/// rest. `MacQueuePanelContainer` reads the stores for each tab.
struct MacQueuePanel<Content: View>: View {
    let tab: SidePaneTab
    let onSelectTab: (SidePaneTab) -> Void
    var lead: SidePaneLead = .none
    let onClose: () -> Void
    var width: CGFloat = 420
    @ViewBuilder
    let content: () -> Content

    var body: some View {
        VStack(spacing: 0) {
            head
            content()
                .frame(
                    maxWidth: .infinity,
                    maxHeight: .infinity,
                    alignment: .top
                )
        }
        .frame(width: width)
        .frame(maxHeight: .infinity)
        // Sidebar chrome: one surface step above the content beside it, and
        // a hairline on the docked edge.
        .background(Theme.surface)
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(Theme.hairline)
                .frame(width: 1)
        }
    }

    /// Where `tab` sits among the control's segments, which list every tab.
    private static func index(of tab: SidePaneTab) -> Int {
        guard let index = SidePaneTab.allCases.firstIndex(of: tab) else {
            preconditionFailure("the side pane's control lists every tab")
        }
        return index
    }

    private var head: some View {
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
            Button(action: action) {
                Image(systemName: "plus")
                    .themeIcon(.large)
                    .frame(
                        width: ThemeSize.hitTarget,
                        height: ThemeSize.hitTarget
                    )
                    .contentShape(Rectangle())
            }
            .buttonStyle(IconHoverButtonStyle().accentFocus())
            .help("New playlist…")
            .accessibilityLabel("New playlist…")
        case .back(let to, let action):
            MacBackButton(to: to, action: action)
        }
    }
}
