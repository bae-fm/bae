import AppKit
import BaeKit
import SwiftUI

@testable import bae

/// bae's look in a shot, as the design boards draw: dark, Midnight, Pink, the
/// time stopped at `ShotClock.now`, and bae's own alerts and layers. A scene
/// whose board shows another look stages it itself.
@MainActor
struct BaeShotLook: ShotLook {
    func dress(_ view: AnyView, framedAs frame: ShotFrame) -> any ShotDressing {
        BaeShotDressing(view, framedAs: frame)
    }
}

/// bae's look for one render: its preferences suite and its stack of alerts
/// and layers.
@MainActor
private final class BaeShotDressing: ShotDressing {
    private static let mode = AppearanceMode.dark
    private static let tone = SurfaceTone.midnight
    private static let accent = AccentChoice.pink

    let rootView: AnyView
    private let suite: String
    private let defaults: UserDefaults
    // A layer that fades in would be captured partway.
    private let modals = ModalWindows(fades: false)

    init(_ view: AnyView, framedAs frame: ShotFrame) {
        // One suite for each render in each process, so a capture that runs
        // at the same time as this one never shares or removes it.
        let suite =
            "fm.bae.appearance-shots.\(ProcessInfo.processInfo.processIdentifier).\(UUID().uuidString)"
        guard let defaults = UserDefaults(suiteName: suite) else {
            preconditionFailure("Cannot create screenshot preferences")
        }
        defaults.set(Self.mode.rawValue, forKey: "appearance.mode")
        defaults.set(Self.accent.rawValue, forKey: "appearance.accent")
        defaults.set(Self.tone.rawValue, forKey: "appearance.tone")
        self.suite = suite
        self.defaults = defaults

        // The bugs a shot's views report go to the crash reporter, which no
        // shot starts.
        let bugReports = BugReports(CoreBugReporter())
        let timed = view.layerEnvironment {
            $0.environment(\.clockNow, ShotClock.now)
                .environment(bugReports)
        }
        let titled =
            if case .window(_, .themed) = frame {
                AnyView(timed.themedWindowTitleBar())
            }
            else {
                AnyView(timed)
            }
        rootView = AnyView(
            titled
                .appAppearance()
                // Each layer over the view reads the shot's preferences and
                // stays still as the view does.
                .layerEnvironment { content in
                    content
                        .defaultAppStorage(defaults)
                        .shotStill()
                }
                .modalWindows(modals)
        )
    }

    func dress(_ window: NSWindow) {
        window.appearance = NSAppearance(named: .darkAqua)
        window.holdsSpinnersStill = true
    }

    /// An alert or a layer is a separate window over the scene's, so the
    /// element list adds its elements where it draws them over that window. A
    /// board reads a layer before the player, which is the last element the
    /// window's own content holds, and an alert or an open menu after
    /// everything. The window under a layer that another layer covers is out
    /// of reach, as a board lists it.
    var overlays: [ShotOverlay] {
        let layers = modals.layerPanels
        return modals.panels.map { panel in
            let covered = layers.contains {
                modals.parent(of: $0) === panel
                    && modals.cover(ofLayer: $0) == .presentingView
            }
            let reading: ShotOverlay.Reading =
                if covered {
                    .unlisted
                }
                else if layers.contains(panel) && !ShotMenuWindows.holds(panel)
                {
                    .before { tree, root in
                        Self.playerStart(in: tree, root: root)
                    }
                }
                else {
                    .last
                }
            return ShotOverlay(panel: panel, reading: reading)
        }
    }

    func undress() {
        defaults.removePersistentDomain(forName: suite)
    }

    /// The index of the first element of the player, which the window's
    /// content holds after everything but an open menu: the part of the window
    /// that holds the button naming what plays, or else its last part.
    private static func playerStart(
        in tree: AccessibilityTree,
        root: Int?
    ) -> Int? {
        let nodes = tree.nodes
        guard let root else { return nil }
        let parts = nodes.indices.filter { nodes[$0].parent == root }
        func part(of index: Int) -> Int? {
            var current = index
            while let parent = nodes[current].parent, parent != root {
                current = parent
            }
            return nodes[current].parent == root ? current : nil
        }
        let playing = nodes.indices.first {
            nodes[$0].role == NSAccessibility.Role.button.rawValue
                && nodes[$0].controlName?.hasPrefix("Now playing") == true
        }
        return playing.flatMap(part(of:)) ?? parts.last
    }
}
