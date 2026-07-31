using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The one place bridge values become the mapping pane's plain model. Every
/// question the pane then asks — the counts, the exclusion, the link glyph, the
/// reconciliation arguments — is <see cref="MappingPaneModel"/>'s, over types
/// that carry no FFI, which is what lets that half be tested off the native
/// bridge.
///
/// Nothing here decides anything: each field is copied across, and the variants
/// map one-to-one onto core's.
/// </summary>
internal static class MappingPaneProjection
{
    /// <summary>The slot table as picking a release computed it, projected onto
    /// the pane's rows. The edit is what supplies each row's typed title and
    /// artist, and it lines up positionally with the slots by construction —
    /// row <c>i</c> edits edit-row <c>i</c>.</summary>
    internal static MappingPaneModel FromSlots(BridgeSlotTable slots, IReadOnlyList<BridgeRawTrackEdit> tracks) =>
        new(
            slots.Rows.Select((slot, index) => Row(slot, index < tracks.Count ? tracks[index] : null)),
            Reconciliation(slots.Reconciliation));

    /// <summary>
    /// The rows of an import with no release mapped onto it. bae-core computes
    /// a slot table against a picked release and this import has none, so there
    /// is nothing to pair and nothing to reconcile — the rows are the edit's
    /// own tracks, as the folder's file tags described them, and they still
    /// write.
    /// </summary>
    internal static MappingPaneModel FromUnidentifiedEdit(IReadOnlyList<BridgeRawTrackEdit> tracks) =>
        new(
            tracks.Select(track => new MappingSlotRow(
                MappingSlotKind.Unidentified,
                position: track.TrackNumber?.ToString(System.Globalization.CultureInfo.CurrentCulture),
                fileId: null,
                fileName: null,
                fileSize: 0,
                localPath: null,
                probedDurationMs: null,
                sourceDurationMs: null,
                MappingSlotSpan.Whole,
                track.Title,
                track.ArtistText)),
            reconciliation: null);

    private static MappingSlotRow Row(BridgeTrackSlot slot, BridgeRawTrackEdit? track) => slot switch
    {
        BridgeTrackSlot.Paired paired => new MappingSlotRow(
            MappingSlotKind.Paired,
            paired.Position,
            FileId(paired.File.Audio),
            paired.File.Name,
            paired.File.Size,
            paired.File.LocalPath,
            paired.File.ProbedDurationMs,
            paired.SourceDurationMs,
            Span(paired.File.Span),
            track?.Title ?? paired.Track.Title,
            track?.ArtistText ?? string.Join(", ", paired.Track.ArtistNames)),
        BridgeTrackSlot.FileOnly fileOnly => new MappingSlotRow(
            MappingSlotKind.FileOnly,
            position: null,
            FileId(fileOnly.File.Audio),
            fileOnly.File.Name,
            fileOnly.File.Size,
            fileOnly.File.LocalPath,
            fileOnly.File.ProbedDurationMs,
            sourceDurationMs: null,
            Span(fileOnly.File.Span),
            track?.Title ?? fileOnly.Track.Title,
            track?.ArtistText ?? string.Join(", ", fileOnly.Track.ArtistNames)),
        BridgeTrackSlot.TrackOnly trackOnly => new MappingSlotRow(
            MappingSlotKind.TrackOnly,
            trackOnly.Position,
            fileId: null,
            fileName: null,
            fileSize: 0,
            localPath: null,
            probedDurationMs: null,
            trackOnly.SourceDurationMs,
            MappingSlotSpan.Whole,
            track?.Title ?? trackOnly.Track.Title,
            track?.ArtistText ?? string.Join(", ", trackOnly.Track.ArtistNames)),
        _ => throw new System.ArgumentOutOfRangeException(nameof(slot), slot, "Unknown track slot"),
    };

    /// <summary>The audio's identity within the release. A slice and the whole
    /// container share it, which is exactly what makes excluding one file drop
    /// every row it backs.</summary>
    internal static string FileId(BridgeAudioFile audio) => audio switch
    {
        BridgeAudioFile.Standalone standalone => standalone.FileId,
        BridgeAudioFile.SheetSlice slice => slice.FileId,
        _ => throw new System.ArgumentOutOfRangeException(nameof(audio), audio, "Unknown audio file"),
    };

    private static MappingSlotSpan Span(BridgeSlotSpan span) => span switch
    {
        BridgeSlotSpan.ContainerStart => MappingSlotSpan.ContainerStart,
        BridgeSlotSpan.ContainerMiddle => MappingSlotSpan.ContainerMiddle,
        BridgeSlotSpan.ContainerEnd => MappingSlotSpan.ContainerEnd,
        _ => MappingSlotSpan.Whole,
    };

    private static MappingReconciliation Reconciliation(BridgeSlotReconciliation reconciliation) => reconciliation switch
    {
        BridgeSlotReconciliation.Agrees agrees =>
            new MappingReconciliation(MappingReconciliationKind.Agrees, agrees.Count, agrees.Count, agrees.Count),
        BridgeSlotReconciliation.MoreFiles more =>
            new MappingReconciliation(MappingReconciliationKind.MoreFiles, 0, more.Files, more.Tracks),
        BridgeSlotReconciliation.MoreTracks more =>
            new MappingReconciliation(MappingReconciliationKind.MoreTracks, 0, more.Files, more.Tracks),
        _ => throw new System.ArgumentOutOfRangeException(
            nameof(reconciliation), reconciliation, "Unknown reconciliation"),
    };

    /// <summary>The directories core decided the roles table shows as one row,
    /// as the plain values the row walk takes.</summary>
    internal static IReadOnlyList<MappingCollapsedDirectory> CollapsedDirectories(BridgeCandidateFiles files) =>
        files.CollapsedDirectories
            .Select(directory => new MappingCollapsedDirectory(
                directory.DirPrefix,
                directory.Kind switch
                {
                    BridgeFileRowKind.Image => MappingFileGroupKind.Image,
                    BridgeFileRowKind.Document => MappingFileGroupKind.Document,
                    _ => MappingFileGroupKind.Other,
                },
                directory.Count,
                directory.TotalSize))
            .ToList();

    /// <summary>The catalog key naming a collapsed directory's contents, back in
    /// core's terms — the group row asks core for its own wording rather than
    /// keeping a second copy of the mapping.</summary>
    internal static BridgeFileRowKind RowKind(MappingFileGroupKind kind) => kind switch
    {
        MappingFileGroupKind.Image => BridgeFileRowKind.Image,
        MappingFileGroupKind.Document => BridgeFileRowKind.Document,
        _ => BridgeFileRowKind.Other,
    };
}
