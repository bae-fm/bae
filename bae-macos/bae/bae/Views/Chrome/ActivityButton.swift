import BaeKit
import SwiftUI

/// The Activity button in the title bar: a small spinner while bytes move, a
/// pause symbol while a queue is held and nothing moves, a warning symbol
/// while a problem the person hasn't dismissed is listed, nothing otherwise.
/// Clicking it opens the Activity panel in a popover.
struct ActivityButton: View {
    @Environment(SyncActivityStore.self)
    private var store
    @Environment(SubsonicStore.self)
    private var subsonicStore
    @Environment(SettingsNavigation.self)
    private var settingsNavigation
    @Environment(\.openSettingsWindow)
    private var openSettings
    @Environment(\.openStorageManager)
    private var openStorageManager
    @Environment(\.launchProblems)
    private var launchProblems
    @Environment(\.libraryLoadError)
    private var libraryLoadError

    @State
    private var listShown = false
    /// The details a problem in the panel opened; the alert shows from here,
    /// since the panel's popover has closed by then.
    @State
    private var openedDetails: ErrorDetails?
    /// The problems the person took out of the panel, by kind, for as long
    /// as they stay listed.
    @State
    private var dismissed: Set<ActivityProblemID> = []

    private var app: [AppProblem] {
        launchProblems.problems + [libraryLoadError?.problem].compactMap { $0 }
    }

    private var problems: ActivityProblems {
        ActivityProblems(
            app: app,
            activity: store.activity,
            error: store.error,
            subsonic: subsonicStore.view.status,
            dismissed: dismissed
        )
    }

    var body: some View {
        let problems = problems
        let moving = store.activity?.moving == true
        let held = store.activity?.held == true
        Group {
            if moving || held || problems.needsAttention {
                Button {
                    listShown = true
                } label: {
                    ActivityGlyph(
                        moving: moving,
                        held: held,
                        needsAttention: problems.needsAttention
                    )
                    .frame(
                        width: ThemeSize.hitTarget,
                        height: ThemeSize.hitTarget
                    )
                    .contentShape(Rectangle())
                }
                .buttonStyle(IconHoverButtonStyle().accentFocus())
                .help(localizedCoreString("core.activity.title"))
                .accessibilityLabel(localizedCoreString("core.activity.title"))
                .macPopover(isPresented: $listShown, arrowEdge: .bottom) {
                    ActivityPanel {
                        ActivityList(
                            problems: problems,
                            app: app,
                            dismissed: $dismissed,
                            onOpenSettings: openSyncSettings,
                            onOpenStorageManager: {
                                listShown = false
                                openStorageManager()
                            },
                            subsonic: subsonicStore.view.status,
                            onOpenSubsonicSettings: openSubsonicSettings
                        )
                    }
                    .environment(
                        \.showErrorDetails,
                        ShowErrorDetailsAction { details in
                            listShown = false
                            // The alert is a sheet, which opens on the main
                            // actor's next turn, after SwiftUI has closed
                            // the popover.
                            Task { @MainActor in
                                openedDetails = details
                            }
                        }
                    )
                }
            }
        }
        .errorDetailsAlert($openedDetails)
        .onChange(of: problems.listed) { _, listed in
            dismissed.formIntersection(listed)
        }
        .onChange(of: moving || held || problems.needsAttention) { _, shown in
            if !shown {
                listShown = false
            }
        }
    }

    private func openSubsonicSettings() {
        listShown = false
        settingsNavigation.open(.subsonic, present: { openSettings() })
    }

    private func openSyncSettings() {
        listShown = false
        settingsNavigation.open(.library, present: { openSettings() })
    }
}

/// The spinner or the pause symbol, the warning, or both. Core never holds
/// a queue while bytes move, so the spinner and the pause never show
/// together.
private struct ActivityGlyph: View {
    let moving: Bool
    let held: Bool
    let needsAttention: Bool

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            if moving {
                MacSpinner()
            }
            if held {
                Image(systemName: "pause.fill")
                    .themeIcon(.large)
            }
            if needsAttention {
                StatusGlyph(.warning)
                    .themeIcon(.large)
            }
        }
    }
}

/// The popover's body: the list inset as the boards draw it (4 above, 14 at
/// the sides and below), scrolling once it is taller than the popover allows.
private struct ActivityPanel<Content: View>: View {
    @ViewBuilder
    let content: Content

    private static var width: CGFloat { 360 }
    private static var maxHeight: CGFloat { 480 }
    private static var inset: CGFloat { 14 }

    /// The list's height, which the popover takes up to `maxHeight`.
    @State
    private var contentHeight: CGFloat?

    var body: some View {
        ScrollView {
            content
                .padding(.top, ThemeSpace.inline)
                .padding(.horizontal, Self.inset)
                .padding(.bottom, Self.inset)
                .frame(width: Self.width)
                .onGeometryChange(for: CGFloat.self, of: { $0.size.height }) {
                    contentHeight = $0
                }
        }
        .scrollBounceBehavior(.basedOnSize)
        .frame(
            width: Self.width,
            height: contentHeight.map { min($0, Self.maxHeight) }
        )
    }
}
