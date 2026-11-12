import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components the panes and sheets are built from: the library
    /// heading, the match code, rows and their notices, progress, record
    /// links, sheet footers, the sort pills, spinners and the join steps.
    static let partsComponentScenes: [ShotScene] =
        libraryHeadingScenes + matchCodeScenes + moreMenuScenes
        + noticeScenes + portFieldScenes + progressScenes
        + progressLineScenes + recordLinksScenes + sheetFooterScenes
        + sortCriteriaScenes + spinnerScenes + stepScenes + itemScenes

    /// `MacLibraryHeading`, at the top of its list and scrolled.
    private static let libraryHeadingScenes: [ShotScene] = [
        libraryHeading("mac.component.library-heading.top", collapse: 0),
        libraryHeading("mac.component.library-heading.scrolled", collapse: 1),
    ]

    private static func libraryHeading(
        _ id: String,
        collapse: Double
    ) -> ShotScene {
        component(id, width: 596, height: 110) {
            MacLibraryHeading(collapseProgress: collapse, fullWidth: false) {
                EmptyView()
            }
            .environment(UiStore())
        }
    }

    /// `MacMatchCode`, the digits.
    private static let matchCodeScenes: [ShotScene] = [
        component("mac.component.match-code.alone", height: 70) {
            MacMatchCode(code: "412 778")
        }
    ]

    /// `MacMoreMenu`, closed.
    private static let moreMenuScenes: [ShotScene] = [
        component("mac.component.more-menu.closed", height: 60) {
            MacMoreMenu {
                MacMenuItem("Details…") {}
            }
        }
    ]

    /// `Notice` in each tone and layout.
    private static let noticeScenes: [ShotScene] = [
        notice("mac.component.notice.neutral", height: 70) {
            Notice(
                title: "This library has no albums yet.",
                advice: nil,
                error: nil,
                tone: .neutral
            )
        },
        notice("mac.component.notice.info", height: 70) {
            Notice(
                title: "A newer version of this release is on Discogs.",
                advice: nil,
                error: nil,
                tone: .info
            )
        },
        notice("mac.component.notice.success", height: 70) {
            Notice(
                title: "Cloud connected.",
                advice: nil,
                error: nil,
                tone: .success
            )
        },
        notice("mac.component.notice.warning", height: 70) {
            Notice(
                title: "Unsent changes will be lost",
                advice: nil,
                error: nil,
                tone: .warning
            )
        },
        notice("mac.component.notice.warning-title-and-subtitle", height: 90) {
            Notice(
                title: "Mara Quill’s phone is out of date",
                advice: "Update bae on it to keep syncing.",
                error: nil,
                tone: .warning
            )
        },
        notice("mac.component.notice.danger", height: 70) {
            Notice(
                title: "Could not reach the cloud.",
                advice: nil,
                error: nil
            )
        },
        notice("mac.component.notice.danger-retry", height: 70) {
            Notice(
                title: "Could not reach the cloud.",
                advice: nil,
                error: nil,
                action: ErrorAction(perform: {})
            )
        },
        notice("mac.component.notice.danger-details-and-retry", height: 70) {
            Notice(
                title: "Could not reach the cloud.",
                advice: nil,
                error: DisplayError(
                    line: "Could not reach the cloud.",
                    detail: "connection refused"
                ),
                action: ErrorAction(perform: {})
            )
        },
        notice(
            "mac.component.notice.danger-actions-below-long-text",
            height: 120
        ) {
            Notice(
                title:
                    "Could not reach the cloud. Your changes stay on this device and go up once it is back; nothing is lost in the meantime.",
                advice: nil,
                error: DisplayError(
                    line: "Could not reach the cloud.",
                    detail: "connection refused"
                ),
                action: ErrorAction(perform: {}),
                buttons: .below
            )
        },
        notice("mac.component.notice.danger-title-and-subtitle", height: 90) {
            Notice(
                title: "bae could not open your library",
                advice: "Check your connection, then quit and reopen bae.",
                error: nil
            )
        },
        notice("mac.component.notice.warning-quiet-actions-below", height: 90) {
            Notice(
                title: "Dropbox signed this device out.",
                advice: nil,
                error: nil,
                tone: .warning,
                look: .quiet,
                action: ErrorAction("Open Sync Settings", perform: {}),
                buttons: .below,
                onDismiss: {}
            )
        },
        notice("mac.component.notice.danger-quiet-with", height: 70) {
            Notice(
                title: "Could not close the library",
                advice: nil,
                error: nil,
                look: .quiet,
                onDismiss: {}
            )
        },
        notice("mac.component.notice.danger-centred-a-failed-view", height: 130)
        {
            Notice(
                title: "Could not read your library",
                advice: nil,
                error: DisplayError(
                    line: "Could not read your library",
                    detail: "database error"
                ),
                look: .centered,
                action: ErrorAction(perform: {})
            )
        },
    ]

    /// One notice, the state `id` names.
    private static func notice(
        _ id: String,
        height: CGFloat,
        notice: @escaping @MainActor () -> Notice
    ) -> ShotScene {
        component(id, height: height) {
            notice().frame(width: 380)
        }
    }

    /// `MacPortField` in a row, and with a port the server can't use.
    private static let portFieldScenes: [ShotScene] = [
        component("mac.component.port-field.port", height: 90) {
            ShotBox {
                LabeledContent("Port") {
                    MacPortField(value: "4533", onCommit: { _ in })
                }
            }
        },
        component("mac.component.port-field.error", height: 100) {
            ShotBox {
                HStack(alignment: .firstTextBaseline) {
                    VStack(alignment: .leading, spacing: ThemeSpace.line) {
                        Text("Port")
                        Text("Another app is using this port")
                            .themeText(.detail)
                            .foregroundStyle(Theme.danger)
                    }
                    Spacer(minLength: 0)
                    MacPortField(
                        value: "4533",
                        invalid: true,
                        onCommit: { _ in }
                    )
                }
            }
        },
    ]

    /// `MacProgress`, a step's name over its bar.
    private static let progressScenes: [ShotScene] = [
        component("mac.component.progress.length-unknown", height: 60) {
            MacProgress(title: "Uploading…", fraction: nil).frame(width: 380)
        },
        component("mac.component.progress.length-known", height: 60) {
            MacProgress(title: "Uploading…", fraction: 0.3).frame(width: 380)
        },
    ]

    /// `MacProgressLine`, a phase, a bar and a count on one line.
    private static let progressLineScenes: [ShotScene] = [
        component("mac.component.progress-line.with-a-count", height: 50) {
            MacProgressLine(
                "Identifying",
                progress: 112 / 130,
                detail: "112 / 130"
            )
            .themeText(.body)
            .frame(width: 380)
        },
        component("mac.component.progress-line.no-count", height: 50) {
            MacProgressLine("Reading files…", progress: 0.42)
                .themeText(.body)
                .frame(width: 380)
        },
        component("mac.component.progress-line.no-known-length", height: 50) {
            MacProgressLine("Importing…", progress: nil)
                .themeText(.body)
                .frame(width: 380)
        },
        component("mac.component.progress-line.upload", height: 50) {
            MacProgressLine(
                "Uploading",
                bar: ProgressTrackBar(progress: 0.15, tint: Theme.activity)
            )
            .themeText(.body)
            .frame(width: 380)
        },
        component("mac.component.progress-line.long-phase", height: 50) {
            MacProgressLine(
                "A phase named at such length that it has to yield to the bar",
                progress: 0.6
            )
            .themeText(.body)
            .frame(width: 200)
        },
    ]

    /// `MacRecordLinks`, the catalogs a release is described by.
    private static let recordLinksScenes: [ShotScene] = [
        component("mac.component.record-links.two-catalogs", height: 50) {
            MacRecordLinks(records: PreviewData.releaseRecordsPair)
                .frame(width: 380, alignment: .leading)
        },
        component("mac.component.record-links.wrapped", height: 90) {
            MacRecordLinks(records: PreviewData.releaseRecordsEveryCatalog)
                .frame(width: 300, alignment: .leading)
        },
    ]

    /// `MacSheetFooter`, a sheet's last row.
    private static let sheetFooterScenes: [ShotScene] = [
        component("mac.component.sheet-footer.plain", height: 60) {
            MacSheetFooter {
                Button("Cancel") {}
                Button("Set up") {}.buttonStyle(PrimaryButtonStyle())
            }
            .frame(width: 380)
        },
        component("mac.component.sheet-footer.split", height: 60) {
            MacSheetFooter {
                Button("New folder…") {}
            } trailing: {
                Button("Cancel") {}
                Button("Set up") {}.buttonStyle(PrimaryButtonStyle())
            }
            .frame(width: 380)
        },
    ]

    /// `MacSortCriteria`, one pill per sort field.
    private static let sortCriteriaScenes: [ShotScene] = [
        component("mac.component.sort-criteria.one", height: 60) {
            MacSortCriteria(
                criteria: .constant([
                    BridgeSortCriterion(
                        field: .dateAdded,
                        direction: .descending
                    )
                ])
            )
        },
        component("mac.component.sort-criteria.two", height: 60) {
            MacSortCriteria(
                criteria: .constant([
                    BridgeSortCriterion(field: .artist, direction: .ascending),
                    BridgeSortCriterion(field: .year, direction: .descending),
                ])
            )
        },
    ]

    /// `MacSpinner`, alone and with words.
    private static let spinnerScenes: [ShotScene] = [
        component("mac.component.spinner.alone", height: 50) {
            MacSpinner()
        },
        component("mac.component.spinner.with-words", height: 50) {
            MacSpinner("Looking for receivers…")
        },
    ]

    /// `MacStep` and `MacStepActions`, a step of joining.
    private static let stepScenes: [ShotScene] = [
        component("mac.component.step.with-a-code", width: 596, height: 300) {
            MacStep("Tell Ana this code") {
                MacMatchCode(code: "412 778")
            }
        },
        component("mac.component.step-actions.two-buttons", height: 60) {
            MacStepActions {
                Button("Cancel") {}
                Button("Join") {}.buttonStyle(PrimaryButtonStyle())
            }
            .frame(width: 380)
        },
    ]

    /// `RowItem`, a row's lead, text and trailing end.
    private static let itemScenes: [ShotScene] = [
        component("mac.component.item.icon-text-trailing", height: 60) {
            RowItem {
                StatusGlyph(.danger, outline: true).themeIcon(.medium)
            } content: {
                Text("Download failed")
            } trail: {
                Text("2m ago").themeText(.fine).foregroundStyle(.secondary)
            }
            .frame(width: 380)
        },
        component("mac.component.item.empty-lead-keeps-the-edge", height: 100) {
            VStack(alignment: .leading, spacing: ThemeSpace.group) {
                RowItem {
                    StatusGlyph(.warning, outline: true).themeIcon(.medium)
                } content: {
                    Text("Dropbox signed this device out.")
                }
                RowItem {
                } content: {
                    Text("01 Harbor Lights.flac")
                } trail: {
                    Text("34.1 MB").themeText(.fine).foregroundStyle(.secondary)
                }
            }
            .frame(width: 380)
        },
        component("mac.component.item.two-lines", height: 70) {
            RowItem {
                StatusGlyph(.info, outline: true).themeIcon(.medium)
            } content: {
                VStack(alignment: .leading, spacing: ThemeSpace.line) {
                    Text("3 uploading")
                    Text("96 MB of 221 MB")
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                }
            }
            .frame(width: 380)
        },
    ]
}

extension MacProgress where Bar == ProgressTrackBar {
    /// A step whose bar fills to `fraction`: 0...1 for a determinate fill,
    /// nil for the marching pill.
    init(title: String, fraction: Double?) {
        self.init(title: title, bar: ProgressTrackBar(progress: fraction))
    }
}
