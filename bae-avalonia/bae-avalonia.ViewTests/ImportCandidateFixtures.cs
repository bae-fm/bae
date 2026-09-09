using System;
using uniffi.bae_bridge;

namespace Bae.Desktop.ViewTests;

/// <summary>One folder as its own read describes it: picked as a release, with
/// a title and a year typed over what that release states, two measured
/// tracks, and a chosen cover. The pane tests draw it and the store tests hand
/// it back over a subscription, so it lives here rather than with either.
/// </summary>
internal static class ImportCandidateFixtures
{
    internal const string CandidateKey = "/Music/Incoming/Album";

    internal static readonly BridgeAudioFormat SourceAudio = new(
        Codec: "FLAC",
        SampleRateHz: 44_100,
        BitsPerSample: 16,
        BitrateKbps: null,
        Channels: 2);

    /// <summary>The folder picked as a release, with the release's own
    /// provenance.</summary>
    internal static BridgeImportCandidateDetail Detail(
        BridgeImportFailure? failure = null,
        BridgeTriageImportStatus? importStatus = null) =>
        Detail(
            new BridgeMetadataProvenance.ExternalRelease(
                BridgeMetadataSource.MusicBrainz,
                "rel-1",
                []),
            failure: failure,
            importStatus: importStatus);

    internal static BridgeImportCandidateDetail Detail(
        BridgeMetadataProvenance? metadataProvenance,
        BridgeImportFailure? failure = null,
        BridgeRawReleaseEdit? edit = null,
        ulong metadataRevision = 1,
        BridgeTriageImportStatus? importStatus = null,
        BridgeLookupChoices? lookupChoices = null,
        string key = CandidateKey,
        string audioIdentity = "mapping-pane-audio") =>
        new(
            Candidate: new BridgeFolderCandidate(
                Combination: null,
                CompositionAction: null,
                SourceFileEditsAllowed: true,
                FolderPath: key,
                SourceFolderName: "Album",
                WatchedFolderPath: "/Music/Incoming",
                Files: new BridgeCandidateFiles(
                    audioIdentity,
                    Array.Empty<BridgeCandidateFile>(),
                    new BridgeCandidateSourceAudio(
                        new BridgeSourceAudioSummary.Uniform(
                            new BridgeSourceAudioDescriptor(
                                BridgeSourceAudioLayout.File,
                                SourceAudio)),
                        Array.Empty<BridgeFileInfo>())),
                TrackCount: 2,
                Skipped: false,
                IsAdded: false),
            Actionable: true,
            ResumedIdentifyState: new BridgeIdentifyState.Idle(),
            Row: Row(metadataProvenance, importStatus, key),
            Release: null,
            PickedLibraryStatus: null,
            FileEvidence: Array.Empty<BridgeFileEvidence>(),
            MetadataDraft: edit
                ?? new BridgeRawReleaseEdit(
                    "Typed Over The Release",
                    ArtistAssignments(),
                    "1991",
                    new BridgeRawPressingEdit(
                        "1996", "CD", "Label Name", "CAT-1", "UK", string.Empty),
                    Array.Empty<BridgeRawTrackEdit>()),
            MetadataDraftIsBlank: edit is not null
                && string.IsNullOrEmpty(edit.AlbumTitle),
            MetadataProvenance: metadataProvenance,
            MetadataAuthor: metadataProvenance is null
                ? BridgeMetadataAuthor.Nobody
                : BridgeMetadataAuthor.User,
            MetadataRevision: metadataRevision,
            InitialMetadataSource: BridgeDefaultImportMetadataSource.None,
            Mapping: new BridgeMappingTable(
                Array.Empty<BridgeMappingImage>(),
                new[]
                {
                    new BridgeMappingTrackSection(
                        new BridgeTrackSide.Flat(),
                        HeaderKey: null,
                        new BridgeMappingTrackSectionContent.Tracks(new[]
                        {
                            TrackRow("01.flac", "Track One"),
                            TrackRow("02.flac", "Track Two"),
                        })),
                },
                Array.Empty<BridgeMappingFileRow>(),
                Reconciliation: null),
            Cover: new BridgeCoverChoice(
                new BridgeCoverSelection.ReleaseImage("cover.jpg"),
                new BridgeCoverImageSource.Local("/Music/Incoming/Album/cover.jpg"),
                new BridgeCoverImageSource.Local("/Music/Incoming/Album/cover.jpg")),
            Signals: null,
            LookupChoices: lookupChoices ?? LookupChoiceEdits.Untouched(),
            Failure: failure,
            // This fixture has not visited Find Online or entered a query.
            Session: new BridgeCandidateSession(
                BridgeMetadataPresentation.Draft,
                new BridgeSearchForm(BridgeSearchTab.General, "", "", "", ""),
                null));

    internal static BridgeTriageRow Row(
        BridgeMetadataProvenance? metadataProvenance,
        BridgeTriageImportStatus? importStatus,
        string key = CandidateKey) => new(
        CandidateKey: key,
        FolderName: "Album",
        WatchedFolderPath: "/Music/Incoming",
        DisplayPath: "Album",
        ResolvedBoundaries: Array.Empty<BridgeResolvedFolderReleaseBoundary>(),
        CombineAncestorKey: null,
        Actionable: true,
        Placement: new BridgeTriagePlacement.Ready(),
        SkipAction: BridgeTriageSkipAction.Skip,
        Actions: [BridgeCandidateAction.ImportReady, BridgeCandidateAction.Identify, BridgeCandidateAction.UseFileMetadata, BridgeCandidateAction.ClearMetadata, BridgeCandidateAction.Skip],
        Matched: null,
        MetadataSummary: null,
        CoverThumbnail: null,
        Selectable: true,
        ImportStatus: importStatus,
        MetadataProvenance: metadataProvenance);

    internal static BridgeRawReleaseEdit BlankEdit() => new(
        string.Empty,
        Array.Empty<BridgeArtistAssignment>(),
        string.Empty,
        new BridgeRawPressingEdit(
            string.Empty,
            string.Empty,
            string.Empty,
            string.Empty,
            string.Empty,
            string.Empty),
        Array.Empty<BridgeRawTrackEdit>());

    internal static BridgeTrackMapping TrackRow(string fileId, string title) =>
        new(
            new BridgeMappingSource.File(new BridgeMappingFile(
                FileId: fileId,
                Name: fileId,
                Size: 1024,
                LocalPath: $"/Music/Incoming/Album/{fileId}",
                PreviewTarget: new BridgePreviewTarget(
                    $"/Music/Incoming/Album/{fileId}", 0, null),
                DurationMs: 180_000,
                AudioFormat: SourceAudio,
                Role: BridgeMappingRole.Audio,
                Alternatives: Array.Empty<BridgeFileRoleChoice>(),
                RoleChoice: null)),
            new BridgeMappingBecomes.Track(
                new BridgeRawTrackEdit(
                    fileId,
                    title,
                    new BridgeTrackArtistAssignments.Explicit(ArtistAssignments()),
                    1,
                    null,
                    new BridgeAudioFile.Standalone(fileId)),
                Position: "1",
                NamedBySource: true),
            DurationMs: 180_000);

    internal static BridgeArtistAssignment[] ArtistAssignments() =>
    [
        new BridgeArtistAssignment.New(
            new BridgeNewArtistSeed("Artist Name", null, null, null)),
    ];

    internal static BridgeReleaseUserEdit FileTagsEdit() => new(
        "Album Title",
        ArtistAssignments(),
        1991,
        new BridgePressingEdit(1996, "CD", "Label Name", "CAT-1", "UK", null),
        Array.Empty<BridgeTrackUserEdit>());
}
