import BaeKit
import Foundation

@testable import bae

extension PreviewData {
    // MARK: - Config and edit-metadata fixtures

    /// The choices the Add menus of a shot's pressing offer. The menus are
    /// closed in every shot, so one fixed set stands for each draft's.
    static let pressingOffers = BridgePressingOffers(
        media: [.cd, .vinyl, .cassette],
        discogsDetails: [.lp, .remastered]
    )

    /// The album artists of the metadata-editor drafts, as the editor's read
    /// answers the field: one library artist and one new artist.
    static let editMetadataAlbumArtists = BridgeArtistField(
        standing: .someNew(count: 1),
        credits: [
            BridgeStandingCredit(
                credited: pickedArtist("Artist Name", artistId: "artist-1"),
                standing: .library
            ),
            BridgeStandingCredit(
                credited: artistCredit("New Artist Name"),
                standing: .new
            ),
        ]
    )

    /// A raw release edit for metadata-editor shots: the album and
    /// pressing fields plus `trackCount` tracks. Blank track artists
    /// exercise the album-artist inheritance path.
    static func editMetadataDraft(
        trackCount: Int,
        blankTrackArtists: Bool = true
    ) -> BridgeRawReleaseEdit {
        BridgeRawReleaseEdit(
            albumTitle: "Album Title",
            albumArtistAssignments: editMetadataAlbumArtists.credits.map(
                \.credited
            ),
            albumYear: "1983",
            pressing: BridgeRawPressingEdit(
                year: "1997",
                labels: [
                    BridgeLabelRow(
                        label: BridgeRawLabelEdit(
                            name: "Label A",
                            catalogNumber: "AB 100"
                        ),
                        place: .first
                    ),
                    BridgeLabelRow(
                        label: BridgeRawLabelEdit(
                            name: "Label B",
                            catalogNumber: "CL 719"
                        ),
                        place: .last
                    ),
                ],
                barcodes: [.alone("000000000000")],
                facts: PreviewData.pressingFacts(
                    country: "US",
                    media: PreviewData.media(.cd)
                )
            ),
            tracks: (1...trackCount)
                .map { n in
                    BridgeRawTrackEdit(
                        id: "t-\(n)",
                        title: "Track Title \(n)",
                        artistAssignments: blankTrackArtists
                            ? .albumArtists
                            : .explicit(
                                assignments: [
                                    artistCredit("Track Artist \(n)")
                                ]
                            ),
                        file: .standalone(fileId: "\(n).flac")
                    )
                }
        )
    }

    /// A compilation's worth of album artists as the editor's read answers
    /// the field: ten library artists and two new ones.
    static let manyAlbumArtists = BridgeArtistField(
        standing: .someNew(count: 2),
        credits: (1...10)
            .map { n in
                BridgeStandingCredit(
                    credited: pickedArtist(
                        "Artist Name \(n)",
                        artistId: "artist-\(n)"
                    ),
                    standing: .library
                )
            } + [
                BridgeStandingCredit(
                    credited: artistCredit("New Artist One"),
                    standing: .new
                ),
                BridgeStandingCredit(
                    credited: artistCredit("New Artist Two"),
                    standing: .new
                ),
            ]
    )

    /// A draft credited to `manyAlbumArtists`, so the header has to
    /// summarize rather than list.
    static func manyAlbumArtistsDraft() -> BridgeRawReleaseEdit {
        var draft = editMetadataDraft(trackCount: 3)
        draft.albumArtistAssignments = manyAlbumArtists.credits.map(\.credited)
        return draft
    }

    /// A release of `trackCount` tracks as its editor opens on it.
    static func releaseEdit(trackCount: Int) -> PreviewReleaseEdit {
        let edit = editMetadataDraft(trackCount: trackCount)
        let format = BridgeAudioFormat(
            codec: "FLAC",
            sampleRateHz: 44_100,
            bitsPerSample: 16,
            bitrateKbps: nil,
            channels: 2
        )
        let draft = BridgeReleaseEditDraft(
            cover: nil,
            sourceAudio: .uniform(
                descriptor: BridgeSourceAudioDescriptor(
                    layout: .file,
                    format: format
                )
            ),
            sides: [
                BridgeTrackTableSide(
                    side: .flat,
                    runs: edit.tracks.enumerated()
                        .map { index, track in
                            .track(
                                row: releaseEditRow(
                                    track,
                                    index: index,
                                    source: .file(
                                        name: "Track \(index + 1).flac"
                                    )
                                )
                            )
                        }
                )
            ],
            files: libraryReleaseFiles,
            album: .named(
                albumId: "album-preview",
                line: .titleAndArtists(
                    title: edit.albumTitle,
                    artistLine: .named("Preview Artist")
                )
            ),
            records: [],
            hasRecords: false,
            pressingOffers: pressingOffers,
            albumArtists: editMetadataAlbumArtists,
            changed: false,
            filesToRead: nil,
            invalid: nil
        )
        return PreviewReleaseEdit(values: edit, draft: draft)
    }

    /// The release editor's row of the track `track`, the one at `index`,
    /// naming `source`.
    static func releaseEditRow(
        _ track: BridgeRawTrackEdit,
        index: Int,
        source: BridgeTrackSource
    ) -> BridgeTrackTableRow {
        tableRow(
            track,
            source: source,
            audition: .libraryTrack,
            position: String(index + 1),
            lengthMs: UInt64(180_000 + index * 12_000)
        )
    }

    /// An editor over a library holding `release`.
    @MainActor
    static func releaseEditor(
        holding release: PreviewReleaseEdit = releaseEdit(trackCount: 5)
    ) -> ReleaseEditor {
        ReleaseEditor(
            openReleaseEdit: { _ in
                PreviewReleaseLibrary(release: release).opened()
            }
        )
    }

    static func artistCredit(_ name: String) -> BridgeCredited {
        .typed(name)
    }

    static func pickedArtist(
        _ name: String,
        artistId: String
    ) -> BridgeCredited {
        .picked(
            BridgeExistingArtist(
                artistId: artistId,
                name: name,
                sortName: nil,
                musicbrainzArtistId: nil,
                discogsArtistId: nil
            )
        )
    }

    /// The library an artist field's search reads in shots.
    static func artistAssignmentsLibrary() -> Library {
        Library(artistSearch: { _ in artistSearch() })
    }

    /// Two library artists of one name for an artist field's search,
    /// whatever is typed; the first with another printed name.
    static func artistSearch() -> BridgeListQueries<BridgeArtistSearchResult> {
        .fixed(
            [
                BridgeArtistSearchResult(
                    artist: BridgeExistingArtist(
                        artistId: "artist-1",
                        name: "Artist Name",
                        sortName: "Name, Artist",
                        musicbrainzArtistId: nil,
                        discogsArtistId: nil
                    ),
                    image: nil,
                    creditNames: [
                        BridgeArtistCreditName(
                            artistId: "artist-1",
                            credit: BridgeCreditName(
                                key: "artist alias",
                                name: "Artist Alias"
                            )
                        )
                    ]
                ),
                BridgeArtistSearchResult(
                    artist: BridgeExistingArtist(
                        artistId: "artist-2",
                        name: "Artist Name",
                        sortName: "Name, Artist",
                        musicbrainzArtistId: nil,
                        discogsArtistId: nil
                    ),
                    image: nil,
                    creditNames: []
                ),
            ],
            answers: UnansweredCount()
        )
    }
}

/// A release as its editor opens on it in previews, shots and tests: the
/// values of the form the editor holds, and the draft beside them.
struct PreviewReleaseEdit: Sendable {
    var values: BridgeRawReleaseEdit
    var draft: BridgeReleaseEditDraft
}

/// A release's library as an editor's form commands meet it: opening reads
/// what it holds, and a save becomes that, or never finishes in a library
/// that holds its saves.
actor PreviewReleaseLibrary {
    /// What the library held before anything was saved to it.
    nonisolated let opening: PreviewReleaseEdit
    /// Whether a save never finishes, so an editor stays on its wait.
    nonisolated let holdsSaves: Bool
    private var saved: PreviewReleaseEdit

    init(release: PreviewReleaseEdit, holdsSaves: Bool = false) {
        opening = release
        self.holdsSaves = holdsSaves
        saved = release
    }

    /// A session opened before anything was saved.
    nonisolated func opened() -> OpenedReleaseEdit {
        opened(PreviewReleaseEditSession(release: opening))
    }

    func open() -> PreviewReleaseEdit {
        saved
    }

    func save(_ release: PreviewReleaseEdit) -> String {
        saved = release
        saved.draft.changed = false
        return "album-preview"
    }

    /// Save `session`'s form, then open the session again on what it saved.
    func save(_ session: PreviewReleaseEditSession) async -> SavedReleaseEdit {
        let albumId = save(await session.release)
        let values = await session.replace(with: open())
        return SavedReleaseEdit(albumId: albumId, values: values)
    }

    nonisolated func commands() -> ReleaseEditCommands {
        ReleaseEditCommands(
            open: { _ in
                self.opened(
                    PreviewReleaseEditSession(release: await self.open())
                )
            },
            searchAlbums: { _ in [] },
            catalogApply: { nil }
        )
    }

    /// Open `session`, and return it with the subscription that sends its
    /// drafts.
    nonisolated func opened(_ session: PreviewReleaseEditSession)
        -> OpenedReleaseEdit
    {
        OpenedReleaseEdit(
            values: session.opening.values,
            drafts: session.drafts(),
            form: commands(of: session)
        )
    }

    /// Pick `pick` as the album `session`'s draft files the release under.
    nonisolated func putAlbum(
        _ pick: BridgeAlbumPick,
        in session: PreviewReleaseEditSession
    ) async {
        _ = await session.change {
            $0.draft.album =
                switch pick {
                case .records: .new
                case .new: .pickedNew
                case .album(let albumId):
                    .picked(
                        albumId: albumId,
                        line: .titleAndArtists(
                            title: "Picked album",
                            artistLine: .named("Preview Artist")
                        )
                    )
                }
        }
    }

    /// Move the CUE sheet `sheetFileId` of `session`'s draft to `picked`, one
    /// of the sheet's choices, as core lays it out.
    nonisolated func putSheet(
        _ sheetFileId: String,
        onSide picked: BridgeMediumSide,
        in session: PreviewReleaseEditSession
    ) async {
        _ = await session.change { release in
            release.draft.sides = release.draft.sides?
                .map { side in
                    var side = side
                    side.runs = side.runs.map { run in
                        guard
                            case .sheet(var sheet, let rows) = run,
                            sheet.sheetId == sheetFileId,
                            let choice = sheet.sideChoices.first(where: {
                                $0.side == picked
                            })
                        else { return run }
                        sheet.assigned = choice
                        return .sheet(sheet: sheet, rows: rows)
                    }
                    return side
                }
        }
    }

    /// Bind the FILE line of the CUE sheet `sheetFileId` in `session`'s draft
    /// to the audio file `audioFileId`, one of the line's options: the run
    /// names it, the pop-up checks it, and a save reads it first.
    nonisolated func putSheet(
        _ sheetFileId: String,
        onFile audioFileId: String,
        in session: PreviewReleaseEditSession
    ) async {
        _ = await session.change { release in
            var picked: [String] = []
            release.draft.sides = release.draft.sides?
                .map { side in
                    var side = side
                    side.runs = side.runs.map { run in
                        guard
                            case .sheet(var sheet, let rows) = run,
                            sheet.sheetId == sheetFileId,
                            case .one(var line) = sheet.boundAudio,
                            let option = line.options.first(where: {
                                $0.fileId == audioFileId
                            })
                        else { return run }
                        line.fileId = option.fileId
                        sheet.boundAudio = .one(reference: line)
                        sheet.bound = .describes(
                            container: BridgeMappingContainer(
                                fileId: option.fileId,
                                name: option.name
                            )
                        )
                        picked.append(option.name)
                        return .sheet(sheet: sheet, rows: rows)
                    }
                    return side
                }
            release.draft.filesToRead = picked
        }
    }

    /// What `session` does with each command.
    nonisolated func commands(
        of session: PreviewReleaseEditSession
    ) -> ReleaseEditFormCommands {
        ReleaseEditFormCommands(
            setField: { field, value in
                await session.change { release in
                    switch field {
                    case .albumTitle: release.values.albumTitle = value
                    case .albumYear: release.values.albumYear = value
                    case .pressingYear: release.values.pressing.year = value
                    }
                }
            },
            setLabels: { _ in
                await session.change { _ in }
            },
            setBarcodes: { _ in
                await session.change { _ in }
            },
            setPressingFact: { fact in
                await session.change {
                    $0.values.pressing.facts = bridgeApplyPressingFact(
                        facts: $0.values.pressing.facts,
                        edit: fact
                    )
                }
            },
            setAlbumArtists: { assignments in
                await session.change {
                    $0.values.albumArtistAssignments = assignments
                }
            },
            setTrack: { track in
                await session.change { release in
                    release.values.tracks = release.values.tracks.map {
                        $0.id == track.id ? track : $0
                    }
                }
            },
            apply: { _, _ in await session.change { _ in } },
            setAlbum: { pick in
                await self.putAlbum(pick, in: session)
            },
            setSheetSide: { sheetFileId, side in
                await self.putSheet(sheetFileId, onSide: side, in: session)
            },
            setSheetFile: { sheetFileId, _, audioFileId in
                await self.putSheet(
                    sheetFileId,
                    onFile: audioFileId,
                    in: session
                )
            },
            cancel: { await session.replace(with: await self.open()) },
            save: {
                if self.holdsSaves { try await Task.neverAnswers() }
                return await self.save(session)
            }
        )
    }
}

/// One session's form and draft in a shot library, which each change edits,
/// and the drafts its subscription sends: the draft at once, then the draft
/// after each change.
actor PreviewReleaseEditSession {
    /// The form and draft the session opened on.
    nonisolated let opening: PreviewReleaseEdit
    private(set) var release: PreviewReleaseEdit
    /// The draft the subscription has not sent yet, if any.
    private var unsent: BridgeReleaseEditDraft?
    private var waiting: CheckedContinuation<BridgeReleaseEditDraft?, Never>?

    init(release: PreviewReleaseEdit) {
        opening = release
        self.release = release
        unsent = release.draft
    }

    /// Make `edit` to the form or its draft; returns the form's values.
    func change(_ edit: @Sendable (inout PreviewReleaseEdit) -> Void)
        -> BridgeRawReleaseEdit
    {
        edit(&release)
        release.draft.changed = true
        send(release.draft)
        return release.values
    }

    /// Open the session again on `release`; returns the form's values.
    func replace(with release: PreviewReleaseEdit) -> BridgeRawReleaseEdit {
        self.release = release
        send(release.draft)
        return release.values
    }

    /// The subscription's values.
    nonisolated func drafts() -> PulledValues<BridgeReleaseEditDraft?> {
        PulledValues(read: { await self.next() })
    }

    private func send(_ draft: BridgeReleaseEditDraft) {
        if let waiting {
            self.waiting = nil
            waiting.resume(returning: draft)
        }
        else {
            unsent = draft
        }
    }

    /// The draft the subscription has not sent yet, if there is one;
    /// otherwise wait for the next change and return the draft it makes.
    /// When a task cancels a pull while it waits, `next()` returns `nil`.
    private func next() async -> BridgeReleaseEditDraft? {
        if let unsent {
            self.unsent = nil
            return unsent
        }
        return await withTaskCancellationHandler {
            await withCheckedContinuation { waiting = $0 }
        } onCancel: {
            Task { await self.endWait() }
        }
    }

    private func endWait() {
        waiting?.resume(returning: nil)
        waiting = nil
    }
}
