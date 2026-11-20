import BaeKit
import Foundation

@testable import bae

extension PreviewData {
    // MARK: - Config and edit-metadata fixtures

    /// The catalog number core's read would hand for the draft's summary line:
    /// the first label row that has one.
    static func summaryCatalogNumber(of edit: BridgeRawReleaseEdit) -> String? {
        edit.pressing.labels.first { !$0.catalogNumber.isEmpty }?.catalogNumber
    }

    /// The Add menus' choices core's read would hand beside `edit`.
    static func pressingOffers(for edit: BridgeRawReleaseEdit)
        -> BridgePressingOffers
    {
        pressingOffers(for: edit.pressing)
    }

    /// The Add menus' choices core's read would hand beside `pressing`: every
    /// medium and Discogs detail it does not hold yet.
    static func pressingOffers(
        for pressing: BridgeRawPressingEdit
    ) -> BridgePressingOffers {
        BridgePressingOffers(
            media: bridgeMedia()
                .filter { medium in
                    !pressing.facts.media.contains { $0.medium == medium }
                },
            discogsDetails: bridgeDiscogsDetails()
                .filter {
                    !pressing.facts.discogsDetails.contains($0)
                }
        )
    }

    /// A raw release edit for metadata-editor shots: the album and
    /// pressing fields plus `trackCount` tracks. Blank track artists
    /// exercise the album-artist inheritance path.
    static func editMetadataDraft(
        trackCount: Int,
        blankTrackArtists: Bool = true
    ) -> BridgeRawReleaseEdit {
        BridgeRawReleaseEdit(
            albumTitle: "Album Title",
            albumArtistAssignments: [
                pickedArtist("Artist Name", artistId: "artist-1"),
                artistCredit("New Artist Name"),
            ],
            albumYear: "1983",
            pressing: BridgeRawPressingEdit(
                year: "1997",
                labels: [
                    BridgeRawLabelEdit(
                        name: "Label A",
                        catalogNumber: "AB 100"
                    ),
                    BridgeRawLabelEdit(
                        name: "Label B",
                        catalogNumber: "CL 719"
                    ),
                ],
                barcodes: ["000000000000"],
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

    /// A draft credited to a compilation's worth of album artists, most
    /// of them already in the library and two of them new, so the header
    /// has to summarize rather than list.
    static func manyAlbumArtistsDraft() -> BridgeRawReleaseEdit {
        var draft = editMetadataDraft(trackCount: 3)
        draft.albumArtistAssignments =
            (1...10)
            .map { n in
                pickedArtist("Artist Name \(n)", artistId: "artist-\(n)")
            } + [
                artistCredit("New Artist One"),
                artistCredit("New Artist Two"),
            ]
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
                BridgeReleaseEditTrackSide(
                    side: .flat,
                    headerKey: nil,
                    runs: edit.tracks.enumerated()
                        .map { index, track in
                            .track(
                                row: BridgeReleaseEditTrackRow(
                                    edit: track,
                                    context: releaseEditTrackContext(
                                        trackId: track.id,
                                        index: index,
                                        source: BridgeReleaseEditTrackSource(
                                            fileId: "file-\(index + 1)",
                                            name: "Track \(index + 1).flac",
                                            layout: .file
                                        )
                                    ),
                                    artistStanding: nil
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
            pressingOffers: pressingOffers(for: edit.pressing),
            albumArtistStanding: nil,
            changed: false,
            invalid: nil
        )
        return PreviewReleaseEdit(values: edit, draft: draft)
    }

    /// One track's context beside its form row in the release editor.
    static func releaseEditTrackContext(
        trackId: String,
        index: Int,
        source: BridgeReleaseEditTrackSource
    ) -> BridgeReleaseEditTrackContext {
        BridgeReleaseEditTrackContext(
            trackId: trackId,
            source: source,
            startClock: BridgeDurationClock.length(ms: 0),
            durationClock: BridgeDurationClock.length(
                ms: UInt64(180_000 + index * 12_000)
            ),
            positionText: String(index + 1)
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
/// what it holds, and a save becomes that.
actor PreviewReleaseLibrary {
    /// What the library held before anything was saved to it.
    nonisolated let opening: PreviewReleaseEdit
    private var saved: PreviewReleaseEdit

    init(release: PreviewReleaseEdit) {
        opening = release
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
            setLabels: { labels in
                await session.change { $0.values.pressing.labels = labels }
            },
            setBarcodes: { barcodes in
                await session.change {
                    $0.values.pressing.barcodes = barcodes
                }
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
            },
            cancel: { await session.replace(with: await self.open()) },
            save: { await self.save(session) }
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
