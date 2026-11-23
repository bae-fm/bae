import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Every editor state uses the same Northbound pressing and its CUE image.
    static let macReleaseEditorScenes: [ShotScene] = [
        editorScene(
            "mac.component.release-editor.import",
            actionRow: .importing(commitControls())
        ),
        editorScene(
            "mac.component.release-editor.two-labels",
            secondLabel: true,
            actionRow: .importing(commitControls())
        ),
        editorScene(
            "mac.component.release-editor.import-to-the-cloud",
            syncs: true,
            actionRow: .importing(commitControls())
        ),
        editorScene(
            "mac.component.release-editor.import-with-a-folder-check",
            content: .unlisted,
            actionRow: .importing(
                commitControls(folderCheck: .sourceTracksUnknown)
            )
        ),
        editorScene(
            "mac.component.release-editor.retry-import",
            actionRow: .importing(
                commitControls(
                    importStatus: .error(
                        error: .Diagnostic(
                            category: .import,
                            detail: "decode verification failed for 1 track"
                        )
                    )
                )
            )
        ),
        editorScene(
            "mac.component.release-editor.nothing-to-import",
            content: .blank,
            actionRow: .importing(nil)
        ),
        editorScene(
            "mac.component.release-editor.import-nothing-read",
            content: .unread,
            actionRow: .importing(nil)
        ),
        editorScene(
            "mac.component.release-editor.save",
            actionRow: .saving(saveRow(changed: false))
        ),
        editorScene(
            "mac.component.release-editor.save-changed",
            actionRow: .saving(saveRow(changed: true))
        ),
        editorScene(
            "mac.component.release-editor.save-reading-a-file",
            actionRow: .saving(
                saveRow(
                    changed: true,
                    reading: "Reading Northbound (2019 rip).flac…"
                )
            ),
            editable: false
        ),
        editorScene(
            "mac.component.release-editor.save-album-picker-open",
            actionRow: .saving(saveRow(changed: false)),
            openAlbum: true
        ),
    ]

    static func saveRow(
        changed: Bool,
        reading: String? = nil
    ) -> ReleaseSaveRow {
        ReleaseSaveRow(
            controls: {
                ReleaseEditControls(
                    changed: changed,
                    isBusy: reading != nil,
                    reading: reading,
                    onCancel: {},
                    onSave: {}
                )
            },
            applying: { nil },
            notice: { nil }
        )
    }

    private enum EditorContent { case identified, unlisted, blank, unread }

    private static func editorScene(
        _ id: String,
        syncs: Bool = false,
        content: EditorContent = .identified,
        secondLabel: Bool = false,
        actionRow: ReleaseEditorActionRow,
        editable: Bool = true,
        openAlbum: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 720,
            height: content == .unread ? 480 : 1100,
            images: PreviewData.artImageStore()
        ) {
            let importDraft = isImport(actionRow)
            let release = cueRelease(importDraft ? .importing : .rebound)
            let draft = release.draft
            let identified = content == .identified || content == .unlisted
            let scanned = content != .unread
            let art =
                scanned
                ? try PreviewData.boardImage("cover.jpg", art: "Northbound")
                : nil
            let values = editorValues(
                release.values,
                content: content,
                secondLabel: secondLabel
            )
            let album = identified ? draft.album : nil
            ZStack(alignment: .topLeading) {
                importStores(
                    MacReleaseEditor(
                        actionRow: actionRow,
                        commands: inertEditorCommands,
                        card: ReleaseEditorCard(
                            values: { values },
                            pressingOffers: draft.pressingOffers,
                            albumArtists: identified ? draft.albumArtists : nil,
                            writer: ReleaseFieldWriter { _, _ in },
                            cover: ReleaseCover(
                                content: art.map { .localFile($0.file) },
                                onEdit: scanned ? {} : nil,
                                onDropImage: nil
                            ),
                            sourceAudio: scanned ? draft.sourceAudio : nil,
                            album: album,
                            albumActions: inertAlbumActions,
                            records: identified ? draft.records : nil,
                            pressingUnknown: false
                        ),
                        tracks: editorTracks(
                            draft: draft,
                            content: content,
                            art: art,
                            importDraft: importDraft
                        ),
                        editingCommands: EditingCommitCommands(),
                        editable: editable,
                        searching: false,
                        search: { EmptyView() }
                    ),
                    syncs: syncs
                )
                if openAlbum, let album {
                    albumMenu(album, x: 209, y: 139)
                }
            }
        }
    }

    private static func isImport(_ row: ReleaseEditorActionRow) -> Bool {
        return
            switch row
        {
        case .importing: true
        case .saving: false
        }
    }

    private static func editorTracks(
        draft: BridgeReleaseEditDraft,
        content: EditorContent,
        art: BridgeMappingImage?,
        importDraft: Bool
    ) -> ReleaseEditorTracks {
        let mapping = inertMappingActions()
        let actions = ReleaseTrackTableActions(
            sheetCaption: mapping.sheetCaption,
            audition: importDraft
                ? mapping.audition
                : TrackAuditionActions(
                    preview: { _ in },
                    stopPreview: {},
                    playTrack: { _ in }
                ),
            editTrack: mapping.trackTable.editTrack
        )
        return ReleaseEditorTracks(
            images: art.map { [$0] },
            onOpenImages: { _ in },
            sides: content == .unread
                ? nil : editorSides(draft.sides, content: content),
            files: content == .unread ? nil : draft.files,
            previewingTarget: nil,
            actions: actions
        )
    }

    private static func editorValues(
        _ source: BridgeRawReleaseEdit,
        content: EditorContent,
        secondLabel: Bool
    ) -> BridgeRawReleaseEdit {
        guard content == .identified || content == .unlisted else {
            return blankEdit()
        }
        guard secondLabel else { return source }
        var values = source
        values.pressing.labels = [
            BridgeLabelRow(
                label: BridgeRawLabelEdit(
                    name: "Harbor Records",
                    catalogNumber: "HR-0412"
                ),
                place: .first
            ),
            BridgeLabelRow(
                label: BridgeRawLabelEdit(
                    name: "Tidewater Music",
                    catalogNumber: "TW-88"
                ),
                place: .last
            ),
        ]
        return values
    }

    private static func editorSides(
        _ sides: [BridgeTrackTableSide]?,
        content: EditorContent
    ) -> [BridgeTrackTableSide]? {
        guard content == .blank || content == .unlisted else { return sides }
        func unlisted(_ row: BridgeTrackTableRow) -> BridgeTrackTableRow {
            var row = row
            row.listedLength = nil
            if content == .blank {
                row.track.title = ""
                row.track.artistAssignments = .explicit(assignments: [])
                row.artist = .empty
            }
            return row
        }
        return sides?
            .map { side in
                var side = side
                side.runs = side.runs.map { run in
                    switch run {
                    case .track(let row): .track(row: unlisted(row))
                    case .sheet(let sheet, let rows):
                        .sheet(sheet: sheet, rows: rows.map(unlisted))
                    }
                }
                return side
            }
    }

    private static let inertEditorCommands = ReleaseEditorCommands(
        identifyAutomatically: {},
        search: {},
        metadataCommand: { _ in },
        unlink: {},
        identifyDisabled: false
    )
}
