using System;
using System.Collections.Generic;
using System.Linq;

namespace Bae.Desktop;

/// <summary>
/// The import mapping pane's decision layer, over plain BCL types: the slot
/// rows the pane draws, the commit bar's two counts, what excluding a file
/// removes, which glyph carries a row's pairing state, and the arguments the
/// reconciliation line interpolates.
///
/// Separate from the pane's rendering because none of this is Avalonia's
/// business and none of it may reach for <c>uniffi.bae_bridge</c> — the tests
/// link this file on its own. The pane projects each <c>BridgeTrackSlot</c>
/// into a <see cref="MappingSlotRow"/> once, then asks this type every question
/// it has; nothing here re-derives a pairing, a role, or a tally that bae-core
/// already settled.
/// </summary>
internal sealed class MappingPaneModel
{
    private readonly List<MappingSlotRow> _rows;

    internal MappingPaneModel(IEnumerable<MappingSlotRow> rows, MappingReconciliation? reconciliation)
    {
        _rows = rows.ToList();
        Reconciliation = reconciliation;
    }

    /// <summary>The slot rows, positionally aligned with the edit's track rows:
    /// row <c>i</c> edits edit-row <c>i</c>, which is what makes an index the
    /// only handle either side needs.</summary>
    internal IReadOnlyList<MappingSlotRow> Rows => _rows;

    /// <summary>The tally above the slot table, as core computed it. Null for an
    /// import with no release mapped onto it — there is no tracklist to
    /// reconcile the folder against, so the line is absent rather than
    /// zeroed.</summary>
    internal MappingReconciliation? Reconciliation { get; private set; }

    /// <summary>Rows that will become tracks: the ones carrying a file. A row
    /// the source names but no audio backs writes nothing.</summary>
    internal int WillWriteCount => _rows.Count(row => row.Writes);

    /// <summary>Rows that will be written and have no name yet. The commit bar
    /// states it and never acts on it.</summary>
    internal int UnansweredCount =>
        _rows.Count(row => row.Writes && string.IsNullOrWhiteSpace(row.Title));

    /// <summary>Nothing the pane computes can stop a commit. Stated here as a
    /// value rather than left implicit so the rule is visible where the counts
    /// are, and so a test can hold it.</summary>
    internal static bool CommitEnabled => true;

    /// <summary>Replace the whole table — what a re-prefetch produces after a
    /// sheet binding or a different release changes which slots exist. The
    /// pane rebuilds in place; the surface never closes and reopens.</summary>
    internal void Replace(IEnumerable<MappingSlotRow> rows, MappingReconciliation? reconciliation)
    {
        _rows.Clear();
        _rows.AddRange(rows);
        Reconciliation = reconciliation;
    }

    internal void SetTitle(int index, string title) => _rows[index].Title = title;

    internal void SetArtist(int index, string artist) => _rows[index].ArtistText = artist;

    /// <summary>
    /// Drop every row backed by <paramref name="fileId"/> and report their
    /// indices, ascending, so the caller removes the edit rows that sat at the
    /// same positions. A container backs several rows, so this is a set, not a
    /// row.
    ///
    /// The removal is local on purpose: <c>setFileRole</c> has already
    /// persisted the decision, and re-prefetching to see it would throw away
    /// every title the user has typed.
    /// </summary>
    internal IReadOnlyList<int> Exclude(string fileId)
    {
        var removed = new List<int>();
        for (var index = 0; index < _rows.Count; index++)
        {
            if (_rows[index].FileId == fileId)
            {
                removed.Add(index);
            }
        }
        for (var i = removed.Count - 1; i >= 0; i--)
        {
            _rows.RemoveAt(removed[i]);
        }
        return removed;
    }

    /// <summary>Drop one row — what "Drop" does to a slot the source names and
    /// nobody answered. The caller removes the edit row at the same index.
    /// </summary>
    internal void RemoveAt(int index) => _rows.RemoveAt(index);

    /// <summary>
    /// Point a row at one of the folder's audio files, which flips it to
    /// paired. The row keeps what the source said about it — its position, its
    /// stated length — and the typed title and artist; only the file changes,
    /// because only the file was in question.
    /// </summary>
    internal void Pair(
        int index,
        string fileId,
        string fileName,
        ulong fileSize,
        string localPath,
        ulong? probedDurationMs,
        MappingSlotSpan span)
    {
        var row = _rows[index];
        _rows[index] = new MappingSlotRow(
            MappingSlotKind.Paired,
            row.Position,
            fileId,
            fileName,
            fileSize,
            localPath,
            probedDurationMs,
            row.SourceDurationMs,
            span,
            row.Title,
            row.ArtistText);
    }

    /// <summary>What auditioning a row plays, or null when the row has no audio
    /// behind it to play.</summary>
    internal string? PlayPath(int index) => _rows[index].LocalPath;

    /// <summary>Whether a row is the one currently previewing, given the path
    /// the preview transport reports. Compared by path because that is the only
    /// identity the preview events carry.</summary>
    internal bool IsPlaying(int index, string? previewingPath) =>
        previewingPath is { Length: > 0 } && _rows[index].LocalPath == previewingPath;

    /// <summary>
    /// The character drawn in the link column. The pairing state is a typed
    /// value and the glyph is this UI's choice; a container's slots read as one
    /// unbroken run down the column, and an unpaired row on either side reads as
    /// a broken one.
    /// </summary>
    internal static string LinkGlyph(MappingSlotKind kind, MappingSlotSpan span) => kind switch
    {
        MappingSlotKind.Paired => span switch
        {
            MappingSlotSpan.ContainerStart => "┳",
            MappingSlotSpan.ContainerMiddle => "┃",
            MappingSlotSpan.ContainerEnd => "┻",
            _ => "━",
        },
        MappingSlotKind.FileOnly or MappingSlotKind.TrackOnly => "╌",
        // A row of an import with no release mapped onto it: there are two
        // sides to link only once a release names the tracks.
        _ => string.Empty,
    };

    /// <summary>
    /// The arguments the reconciliation message interpolates. The key itself
    /// comes from <c>bridgeSlotReconciliationKey</c>; only which numbers ride
    /// with it is the caller's to assemble, and it differs per variant.
    /// </summary>
    internal static IReadOnlyDictionary<string, object?> ReconciliationArgs(MappingReconciliation reconciliation) =>
        reconciliation.Kind == MappingReconciliationKind.Agrees
            ? new Dictionary<string, object?> { ["count"] = (long)reconciliation.Count }
            : new Dictionary<string, object?>
            {
                ["files"] = (long)reconciliation.Files,
                ["tracks"] = (long)reconciliation.Tracks,
            };

    /// <summary>
    /// The roles table's rows, walking the folder's files in order and standing
    /// a collapsed directory's files up as one group row at the position of the
    /// first of them. Core decides which directories collapse; this only walks
    /// its decision, so a directory it did not name stays one row per file.
    /// </summary>
    internal static IReadOnlyList<MappingFileRow> FileRows(
        IReadOnlyList<string?> dirPrefixes,
        IReadOnlyList<MappingCollapsedDirectory> collapsed)
    {
        var byPrefix = collapsed.ToDictionary(directory => directory.DirPrefix);
        var emitted = new HashSet<string>();
        var rows = new List<MappingFileRow>();
        for (var index = 0; index < dirPrefixes.Count; index++)
        {
            if (dirPrefixes[index] is { } prefix && byPrefix.TryGetValue(prefix, out var directory))
            {
                if (emitted.Add(prefix))
                {
                    rows.Add(new MappingFileRow(null, directory));
                }
                continue;
            }
            rows.Add(new MappingFileRow(index, null));
        }
        return rows;
    }
}

/// <summary>Whether a slot row's source tracklist and the folder's audio agree
/// about it — bae-core's <c>BridgeTrackSlot</c> variant, plus the one state
/// that has no slot table behind it at all.</summary>
internal enum MappingSlotKind
{
    /// <summary>The source names this track and audio on disk backs it.</summary>
    Paired,

    /// <summary>Audio the source's tracklist does not account for.</summary>
    FileOnly,

    /// <summary>A track the source names with no audio bound to it.</summary>
    TrackOnly,

    /// <summary>An import with no release mapped onto it: the row is one of the
    /// folder's own files as its tags describe it. There is no slot table for
    /// this — bae-core only computes one against a picked release — so the row
    /// carries the edit and nothing else, and it still writes a track.</summary>
    Unidentified,
}

/// <summary>Where a row sits in the run of rows one container is carved into —
/// bae-core's <c>BridgeSlotSpan</c>.</summary>
internal enum MappingSlotSpan
{
    Whole,
    ContainerStart,
    ContainerMiddle,
    ContainerEnd,
}

/// <summary>One row of the slot table. Everything but the two edited fields
/// arrives computed from bae-core; <see cref="Title"/> and
/// <see cref="ArtistText"/> are what the row's fields write back.</summary>
internal sealed class MappingSlotRow
{
    internal MappingSlotRow(
        MappingSlotKind kind,
        string? position,
        string? fileId,
        string? fileName,
        ulong fileSize,
        string? localPath,
        ulong? probedDurationMs,
        ulong? sourceDurationMs,
        MappingSlotSpan span,
        string title,
        string artistText)
    {
        Kind = kind;
        Position = position;
        FileId = fileId;
        FileName = fileName;
        FileSize = fileSize;
        LocalPath = localPath;
        ProbedDurationMs = probedDurationMs;
        SourceDurationMs = sourceDurationMs;
        Span = span;
        Title = title;
        ArtistText = artistText;
    }

    internal MappingSlotKind Kind { get; }

    /// <summary>The source's own position string. Null for a row the source
    /// says nothing about.</summary>
    internal string? Position { get; }

    /// <summary>The audio's identity within the release — what
    /// <c>setFileRole</c> takes, and what excluding matches on. Null for a row
    /// with no audio behind it.</summary>
    internal string? FileId { get; }

    internal string? FileName { get; }

    /// <summary>The whole container's size, even where this row is one slice of
    /// it.</summary>
    internal ulong FileSize { get; }

    internal string? LocalPath { get; }

    internal ulong? ProbedDurationMs { get; }

    internal ulong? SourceDurationMs { get; }

    internal MappingSlotSpan Span { get; }

    internal string Title { get; set; }

    internal string ArtistText { get; set; }

    /// <summary>Whether committing writes a track for this row. A row the
    /// source names with no audio behind it writes nothing.</summary>
    internal bool Writes => Kind != MappingSlotKind.TrackOnly;
}

/// <summary>The job a collapsed directory's files share — bae-core's
/// <c>BridgeFileRowKind</c>.</summary>
internal enum MappingFileGroupKind
{
    Image,
    Document,
    Other,
}

/// <summary>A directory the roles table shows as one row instead of one row per
/// file.</summary>
internal sealed record MappingCollapsedDirectory(
    string DirPrefix,
    MappingFileGroupKind Kind,
    uint Count,
    ulong TotalSize);

/// <summary>One row of the roles table: either one of the folder's files, by its
/// index in the file list, or the group row standing in for a whole collapsed
/// directory. Exactly one of the two is set.</summary>
internal sealed record MappingFileRow(int? FileIndex, MappingCollapsedDirectory? Directory);

/// <summary>Which way the folder's audio and the source's tracklist disagree —
/// bae-core's <c>BridgeSlotReconciliation</c>.</summary>
internal enum MappingReconciliationKind
{
    Agrees,
    MoreFiles,
    MoreTracks,
}

/// <summary>The tally above the slot table, as core computed it.</summary>
internal sealed record MappingReconciliation(
    MappingReconciliationKind Kind,
    uint Count,
    uint Files,
    uint Tracks);
