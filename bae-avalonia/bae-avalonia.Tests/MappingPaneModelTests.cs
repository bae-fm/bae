using System.Collections.Generic;
using System.Linq;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

// The import mapping pane, behaviourally: binding a sheet rebuilds the slot
// table in place, naming an unmatched file moves the commit bar's counts,
// excluding a file drops every row that file backed, nothing the pane computes
// disables the commit, and a slot row plays its own audio.
//
// These are about the pane. What a bound sheet carves, which files a folder
// holds, and how a release reconciles against it are bae-core's, and bae-core's
// own tests hold them.
public sealed class MappingPaneModelTests
{
    private const string Container = "disc.flac";

    private static MappingSlotRow Paired(
        string position, string fileId, string title, MappingSlotSpan span = MappingSlotSpan.Whole,
        ulong probedMs = 200_000, ulong sourceMs = 200_000) =>
        new(MappingSlotKind.Paired, position, fileId, fileId, 1024, $"/folder/{fileId}",
            probedMs, sourceMs, span, title, string.Empty);

    private static MappingSlotRow FileOnly(string fileId, string title) =>
        new(MappingSlotKind.FileOnly, null, fileId, fileId, 1024, $"/folder/{fileId}",
            200_000, null, MappingSlotSpan.Whole, title, string.Empty);

    private static MappingSlotRow TrackOnly(string position, string title) =>
        new(MappingSlotKind.TrackOnly, position, null, null, 0, null,
            null, 200_000, MappingSlotSpan.Whole, title, string.Empty);

    // The twelve slots one bound track sheet carves out of one container: the
    // whole run shares the container's file id, which is what makes excluding
    // it take every row with it.
    private static List<MappingSlotRow> TwelveSlices() =>
        Enumerable.Range(1, 12).Select(number => Paired(
            number.ToString(),
            Container,
            $"Track {number}",
            number == 1 ? MappingSlotSpan.ContainerStart
                : number == 12 ? MappingSlotSpan.ContainerEnd
                : MappingSlotSpan.ContainerMiddle)).ToList();

    private static List<MappingSlotRow> TwelveFiles() =>
        Enumerable.Range(1, 12).Select(number =>
            Paired(number.ToString(), $"{number:00}.flac", $"Track {number}")).ToList();

    private static MappingReconciliation Agrees(uint count) =>
        new(MappingReconciliationKind.Agrees, count, count, count);

    // 1. Binding a sheet rebuilds the slot table: the folder that read as one
    //    unsliced container becomes twelve slots, in place — the pane does not
    //    close and reopen, and the commit bar follows the new shape.
    [Fact]
    public void BindingASheet_RebuildsTheSlotTable()
    {
        var model = new MappingPaneModel(
            new[] { Paired("1", Container, "Disc") },
            new MappingReconciliation(MappingReconciliationKind.MoreTracks, 0, 1, 12));
        Assert.Equal(1, model.WillWriteCount);

        model.Replace(TwelveSlices(), Agrees(12));

        Assert.Equal(12, model.Rows.Count);
        Assert.Equal(12, model.WillWriteCount);
        Assert.Equal(MappingReconciliationKind.Agrees, model.Reconciliation!.Kind);
    }

    // 2. Naming an unmatched file updates the commit bar: thirteen files against
    //    a twelve-track source write thirteen tracks either way, and typing the
    //    thirteenth's title is what clears the unanswered count.
    [Fact]
    public void NamingAnUnmatchedFile_UpdatesTheCommitBar()
    {
        var rows = TwelveFiles();
        rows.Add(FileOnly("13.flac", string.Empty));
        var model = new MappingPaneModel(rows, new MappingReconciliation(MappingReconciliationKind.MoreFiles, 0, 13, 12));

        Assert.Equal(13, model.WillWriteCount);
        Assert.Equal(1, model.UnansweredCount);

        model.SetTitle(12, "Hidden Track");

        Assert.Equal(13, model.WillWriteCount);
        Assert.Equal(0, model.UnansweredCount);
    }

    // 3. Excluding a file removes its slot and restores the count. The edit rows
    //    at the reported indices go with it, which is what keeps the two lists
    //    aligned without either re-reading the other.
    [Fact]
    public void ExcludingAFile_RemovesItsSlotAndRestoresTheCount()
    {
        var rows = TwelveFiles();
        rows.Add(FileOnly("13.flac", string.Empty));
        var model = new MappingPaneModel(rows, new MappingReconciliation(MappingReconciliationKind.MoreFiles, 0, 13, 12));
        Assert.Equal(13, model.WillWriteCount);

        var removed = model.Exclude("13.flac");

        Assert.Equal(new[] { 12 }, removed);
        Assert.Equal(12, model.Rows.Count);
        Assert.Equal(12, model.WillWriteCount);
        Assert.Equal(0, model.UnansweredCount);
    }

    // A container backs a run of rows, so excluding it takes the whole run —
    // one decision about one file, not twelve.
    [Fact]
    public void ExcludingAContainer_RemovesEveryRowItBacks()
    {
        var model = new MappingPaneModel(TwelveSlices(), Agrees(12));

        var removed = model.Exclude(Container);

        Assert.Equal(Enumerable.Range(0, 12), removed);
        Assert.Empty(model.Rows);
        Assert.Equal(0, model.WillWriteCount);
    }

    // 4. Nothing disables the commit — not an unanswered row, not a row the
    //    source names with no audio behind it. Both are stated; neither gates.
    [Fact]
    public void NothingDisablesTheCommit()
    {
        var unanswered = new MappingPaneModel(
            new[] { Paired("1", "01.flac", "Track 1"), FileOnly("02.flac", "   ") },
            new MappingReconciliation(MappingReconciliationKind.MoreFiles, 0, 2, 1));
        Assert.Equal(1, unanswered.UnansweredCount);
        Assert.True(MappingPaneModel.CommitEnabled);

        var missingFile = new MappingPaneModel(
            new[] { Paired("1", "01.flac", "Track 1"), TrackOnly("2", "Track 2") },
            new MappingReconciliation(MappingReconciliationKind.MoreTracks, 0, 1, 2));
        Assert.Equal(1, missingFile.WillWriteCount);
        Assert.True(MappingPaneModel.CommitEnabled);
    }

    // 5. Playing a slot's file works from the slot row: the row names the path
    //    the preview plays, the row playing it is the one that accents, and a
    //    row with no audio behind it offers nothing to play.
    [Fact]
    public void PlayingASlotsFile_WorksFromTheSlotRow()
    {
        var model = new MappingPaneModel(
            new[] { Paired("1", "01.flac", "Track 1"), TrackOnly("2", "Track 2") },
            new MappingReconciliation(MappingReconciliationKind.MoreTracks, 0, 1, 2));

        Assert.Equal("/folder/01.flac", model.PlayPath(0));
        Assert.Null(model.PlayPath(1));

        Assert.True(model.IsPlaying(0, "/folder/01.flac"));
        Assert.False(model.IsPlaying(1, "/folder/01.flac"));
        Assert.False(model.IsPlaying(0, null));
    }

    // The link column: a container's slots read as one unbroken run, a whole
    // file as a single link, and either side of an unpaired row as a broken one.
    [Fact]
    public void LinkGlyph_CarriesThePairingState()
    {
        Assert.Equal("━", MappingPaneModel.LinkGlyph(MappingSlotKind.Paired, MappingSlotSpan.Whole));
        Assert.Equal("┳", MappingPaneModel.LinkGlyph(MappingSlotKind.Paired, MappingSlotSpan.ContainerStart));
        Assert.Equal("┃", MappingPaneModel.LinkGlyph(MappingSlotKind.Paired, MappingSlotSpan.ContainerMiddle));
        Assert.Equal("┻", MappingPaneModel.LinkGlyph(MappingSlotKind.Paired, MappingSlotSpan.ContainerEnd));
        Assert.Equal("╌", MappingPaneModel.LinkGlyph(MappingSlotKind.FileOnly, MappingSlotSpan.Whole));
        Assert.Equal("╌", MappingPaneModel.LinkGlyph(MappingSlotKind.TrackOnly, MappingSlotSpan.Whole));
    }

    // The reconciliation line interpolates a different pair of numbers per
    // variant, so the arguments ride with the key rather than being assembled
    // at each call site.
    [Fact]
    public void ReconciliationArgs_MatchTheVariant()
    {
        var agrees = MappingPaneModel.ReconciliationArgs(Agrees(12));
        Assert.Equal(12L, agrees["count"]);
        Assert.Single(agrees);

        var more = MappingPaneModel.ReconciliationArgs(
            new MappingReconciliation(MappingReconciliationKind.MoreFiles, 0, 13, 12));
        Assert.Equal(13L, more["files"]);
        Assert.Equal(12L, more["tracks"]);
        Assert.False(more.ContainsKey("count"));
    }

    // Choosing a file for an unanswered slot flips it to paired and keeps
    // everything the source said about the row plus whatever has been typed
    // into it.
    [Fact]
    public void ChoosingAFile_PairsTheRowAndKeepsWhatWasTyped()
    {
        var model = new MappingPaneModel(
            new[] { TrackOnly("A1", "Opening") },
            new MappingReconciliation(MappingReconciliationKind.MoreTracks, 0, 0, 1));

        model.Pair(0, "01.flac", "01.flac", 4096, "/folder/01.flac", 199_000, MappingSlotSpan.Whole);

        var row = model.Rows[0];
        Assert.Equal(MappingSlotKind.Paired, row.Kind);
        Assert.Equal("A1", row.Position);
        Assert.Equal("Opening", row.Title);
        Assert.Equal(200_000UL, row.SourceDurationMs);
        Assert.Equal(199_000UL, row.ProbedDurationMs);
        Assert.Equal(1, model.WillWriteCount);
    }

    // A homogeneous directory is one row standing in for all of its files, at
    // the position of the first of them; a directory core did not collapse
    // stays one row per file.
    [Fact]
    public void FileRows_CollapseTheDirectoriesCoreNamed()
    {
        var prefixes = new List<string?> { null, "covers/", "covers/", "covers/", null, "scans/" };
        var collapsed = new List<MappingCollapsedDirectory>
        {
            new("covers/", MappingFileGroupKind.Image, 3, 30_000),
        };

        var rows = MappingPaneModel.FileRows(prefixes, collapsed);

        Assert.Equal(4, rows.Count);
        Assert.Equal(0, rows[0].FileIndex);
        Assert.Equal("covers/", rows[1].Directory!.DirPrefix);
        Assert.Equal(4, rows[2].FileIndex);
        Assert.Equal(5, rows[3].FileIndex);
    }
}
