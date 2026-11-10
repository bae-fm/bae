import BaeKit
import SwiftUI

/// The Activity button in the title bar: a small spinner while bytes move, a
/// warning symbol while a problem the person hasn't dismissed is listed,
/// nothing otherwise. Clicking it opens the Activity panel in a popover.
struct ActivityButton: View {
    @Environment(SyncActivityStore.self)
    private var store
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
            dismissed: dismissed
        )
    }

    var body: some View {
        let problems = problems
        let moving = store.activity?.moving == true
        Group {
            if moving || problems.needsAttention {
                Button {
                    listShown = true
                } label: {
                    ActivityGlyph(
                        moving: moving,
                        needsAttention: problems.needsAttention
                    )
                    .frame(
                        width: ThemeSize.hitTarget,
                        height: ThemeSize.hitTarget
                    )
                    .contentShape(Rectangle())
                }
                .buttonStyle(IconHoverButtonStyle())
                .help(localizedCoreString("core.activity.title"))
                .accessibilityLabel(localizedCoreString("core.activity.title"))
                .popover(isPresented: $listShown, arrowEdge: .bottom) {
                    Form {
                        ActivityList(
                            problems: problems,
                            app: app,
                            dismissed: $dismissed,
                            onOpenSettings: openSyncSettings,
                            onOpenStorageManager: {
                                listShown = false
                                openStorageManager()
                            }
                        )
                    }
                    .formStyle(.grouped)
                    .frame(width: 360)
                    .frame(maxHeight: 480)
                    .environment(
                        \.showErrorDetails,
                        ShowErrorDetailsAction { details in
                            listShown = false
                            // The alert runs modally, so it opens on the
                            // main actor's next turn, after SwiftUI has
                            // closed the popover.
                            Task { @MainActor in
                                ErrorDetailsAlert.show(details)
                            }
                        }
                    )
                }
            }
        }
        .onChange(of: problems.listed) { _, listed in
            dismissed.formIntersection(listed)
        }
        .onChange(of: moving || problems.needsAttention) { _, shown in
            if !shown {
                listShown = false
            }
        }
    }

    private func openSyncSettings() {
        listShown = false
        settingsNavigation.open(.library, present: { openSettings() })
    }
}

/// The spinner, the warning, or both.
private struct ActivityGlyph: View {
    let moving: Bool
    let needsAttention: Bool

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            if moving {
                WaitingIndicator()
            }
            if needsAttention {
                StatusGlyph(.warning)
                    .themeIcon(.large)
            }
        }
    }
}

#if DEBUG
    #Preview("Activity: moving and needing attention") {
        ActivityButton()
            .environment(SyncActivityStore.preview())
            .environment(SettingsNavigation())
            .environment(\.openSettingsWindow, OpenSettingsWindowAction {})
            .padding()
    }

    #Preview("Activity: moving") {
        ActivityButton()
            .environment(SyncActivityStore.preview(needsAttention: false))
            .environment(SettingsNavigation())
            .environment(\.openSettingsWindow, OpenSettingsWindowAction {})
            .padding()
    }

    #Preview("Activity panel: launch and load problems") {
        ActivityListPreview()
            .environment(SyncActivityStore.preview())
            .environment(Sync.stub())
            .environment(Downloads.stub())
            .environment(Outputs.stub())
            .environment(UiStore())
    }

    /// The panel with the sign-in settings problem and a library that failed
    /// to open.
    private struct ActivityListPreview: View {
        @Environment(SyncActivityStore.self)
        private var store
        @State
        private var dismissed: Set<ActivityProblemID> = []

        private let app =
            LaunchProblems(signInSettings: PreviewData.displayErrorWithDetail)
            .problems + [
                LoadError(.libraryOpen, PreviewData.displayErrorWithDetail)
                    .problem
            ]

        var body: some View {
            Form {
                ActivityList(
                    problems: ActivityProblems(
                        app: app,
                        activity: store.activity,
                        error: store.error,
                        dismissed: dismissed
                    ),
                    app: app,
                    dismissed: $dismissed,
                    onOpenSettings: {},
                    onOpenStorageManager: {}
                )
            }
            .formStyle(.grouped)
            .frame(width: 360, height: 480)
        }
    }
#endif
