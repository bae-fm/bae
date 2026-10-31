import AppKit
import BaeKit
import SwiftUI

/// The candidate list's menu trigger: the "more" glyph, marked when a filter
/// hides Found's rows or a scan failed, opening
/// `CandidateListMenuController`'s menu on a press.
///
/// Found's filter entries and the folder scans are read here from
/// `importStore`, so a count moving or a scan stepping re-renders the trigger
/// alone and reaches the menu, open or not.
struct CandidateListMenu: View {
    let watchedFolders: [BridgeWatchedFolder]
    /// Roots with a refresh in flight; their Refresh entry is disabled.
    let refreshingFolders: Set<Data>
    /// Whether the queue has folder groups; without any, Expand All and
    /// Collapse All are disabled.
    let hasGroups: Bool
    let sortOrder: BridgeImportListOrder
    /// The entry Found's rows are narrowed to.
    let pendingFilter: BridgePendingFilter
    /// Holds Found's filter entries, with how many rows each holds, and where
    /// each watched root's scan stands.
    let importStore: ImportStore
    /// Whether the tab on show is Found, the one tab the filter applies to.
    let pendingFilterApplies: Bool
    let actions: CandidateListMenuActions

    private var state: CandidateListMenuState {
        let statuses = importStore.folderScans.statuses
        return CandidateListMenuState(
            watchedFolders: watchedFolders,
            refreshingFolders: refreshingFolders,
            // Each root's scan state, by path. A failed scan also marks the
            // trigger.
            scanStatuses: Dictionary(
                statuses.map { ($0.watchedFolderPath, $0.status) },
                uniquingKeysWith: { first, _ in first }
            ),
            networkFolders: Set(
                statuses.filter(\.onNetworkVolume).map(\.watchedFolderPath)
            ),
            hasGroups: hasGroups,
            sortOrder: sortOrder,
            pendingFilter: pendingFilter,
            pendingFilterEntries: importStore.pendingFilterEntries,
            pendingFilterApplies: pendingFilterApplies
        )
    }

    var body: some View {
        let state = state
        Image(systemName: "ellipsis.circle")
            .themeIcon(ImportFilterBarLayout.glyph)
            .overlay(alignment: .topTrailing) {
                // A filter hiding Found's rows marks the trigger, so a short
                // list never reads as a short queue.
                if pendingFilter != .all && !state.hasFailedScan {
                    Circle()
                        .fill(Color.accentColor)
                        .frame(width: 6, height: 6)
                        .offset(x: 2, y: -2)
                }
                if state.hasFailedScan {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .themeIcon(.badge)
                        .foregroundStyle(Theme.danger)
                        .offset(x: 3, y: -3)
                }
            }
            .filterBarControl()
            .foregroundStyle(.secondary)
            // The trigger above it is the element a person and VoiceOver
            // press.
            .accessibilityHidden(true)
            .overlay {
                CandidateListMenuTrigger(state: state, actions: actions)
            }
    }
}

/// The view a press on opens the menu from, below itself, keeping the menu's
/// controller across renders and handing it each new state.
private struct CandidateListMenuTrigger: NSViewRepresentable {
    let state: CandidateListMenuState
    let actions: CandidateListMenuActions

    func makeCoordinator() -> CandidateListMenuController {
        CandidateListMenuController(state: state, actions: actions)
    }

    func makeNSView(context: Context) -> TriggerView {
        TriggerView(controller: context.coordinator)
    }

    func updateNSView(_: TriggerView, context: Context) {
        context.coordinator.actions = actions
        context.coordinator.show(state)
    }

    /// Opens the menu on mouse down, as a pull-down button does, and to
    /// VoiceOver presents itself as one.
    final class TriggerView: NSView {
        private let controller: CandidateListMenuController

        init(controller: CandidateListMenuController) {
            self.controller = controller
            super.init(frame: .zero)
            let label = String(
                localized: "Sorting, Filtering and Watched Folders"
            )
            toolTip = label
            setAccessibilityElement(true)
            setAccessibilityRole(.menuButton)
            setAccessibilityLabel(label)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) {
            fatalError("init(coder:) has not been implemented")
        }

        override func mouseDown(with _: NSEvent) {
            popUp()
        }

        override func accessibilityPerformPress() -> Bool {
            popUp()
            return true
        }

        override func accessibilityPerformShowMenu() -> Bool {
            popUp()
            return true
        }

        private func popUp() {
            controller.menu.popUp(
                positioning: nil,
                at: NSPoint(x: 0, y: isFlipped ? bounds.maxY : bounds.minY),
                in: self
            )
        }
    }
}

#if DEBUG
    #Preview("Candidate list menu") {
        let importStore = ImportStore()
        importStore.applyPendingFilterEntries(
            PreviewData.pendingFilterEntries()
        )
        importStore.applyFolderScans(
            BridgeFolderScans(
                statuses: [
                    BridgeWatchedFolderScanStatus(
                        watchedFolderPath: Data("/Music/Rips".utf8),
                        watchedFolderName: "Rips",
                        status: .scanning(foundCount: 8),
                        onNetworkVolume: false
                    ),
                    BridgeWatchedFolderScanStatus(
                        watchedFolderPath: Data("/Volumes/Vault".utf8),
                        watchedFolderName: "Vault",
                        status: .failed(
                            error: "The volume could not be reached."
                        ),
                        onNetworkVolume: true
                    ),
                ],
                activity: nil
            )
        )
        return CandidateListMenu(
            watchedFolders: [
                PreviewData.importWatchedFolder,
                BridgeWatchedFolder(
                    path: Data("/Music/Rips".utf8),
                    pathText: "/Music/Rips",
                    name: "Rips"
                ),
                BridgeWatchedFolder(
                    path: Data("/Volumes/Vault".utf8),
                    pathText: "/Volumes/Vault",
                    name: "Vault"
                ),
            ],
            refreshingFolders: [],
            hasGroups: true,
            sortOrder: .newestFirst,
            pendingFilter: .needsYou,
            importStore: importStore,
            pendingFilterApplies: true,
            actions: CandidateListMenuActions(
                onSetSortOrder: { _ in },
                onSetPendingFilter: { _ in },
                onAddFolder: {},
                onSetAllGroupsExpanded: { _ in },
                onRefreshFolder: { _ in },
                onRemoveFolder: { _ in }
            )
        )
        .padding()
        .windowBackground()
    }
#endif
