using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Interactivity;
using Avalonia.LogicalTree;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// The mapping table, as it renders: a track sheet heads the group of entries it
/// carves, its two decisions sit on the header, and the right half states what
/// each row becomes — the same table before a release is picked as after it.
///
/// What a bound sheet carves, which rows a file backs and what the tally reads
/// are bae-core's, and bae-core's own tests hold them. These are about what the
/// pane draws over that answer.
/// </summary>
public sealed class ImportMappingTableTests
{
    private const string SheetId = "disc.cue";
    private const string ContainerPath = "/folder/disc.flac";

    // ── The table, rendered ──────────────────────────────────────────────────

    // Before a release is picked every audio row says so, in the same table and
    // the same columns a pick fills: there is no identify⇄confirm layout flip.
    [AvaloniaFact]
    public void BeforeAPickTheBecomesColumnStatesTheOpenQuestion()
    {
        var table = Build(SheetTable(Disc(1)));

        var awaiting = table
            .GetLogicalDescendants()
            .OfType<TextBlock>()
            .Count(text => text.Text == Loc.Core("ui.import.becomes.awaiting_pick"));

        Assert.Equal(2, awaiting);
        // The header, the sheet's own row, and one row per entry.
        Assert.Equal(4, Rows(table).Count);
        // Nothing is picked, so there is no track on any row to edit.
        Assert.Empty(table.GetLogicalDescendants().OfType<TextBox>());
    }

    // A sheet is one group row over its entries, and the header states why this
    // one is on no audio at all. Every row starts at the same leading edge — a
    // sheet's entries are its rows, which the group above them already says.
    [AvaloniaFact]
    public void ASheetHeadsTheGroupOfEntriesItCarves()
    {
        var table = Build(SheetTable(Disc(1)));
        var rows = Rows(table);

        Assert.All(rows.Skip(1), row => Assert.Equal(0, SourceCell(row).Margin.Left));
        Assert.Contains(
            table.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == Loc.Core("ui.import.sheet.asked_for", "names", "disc.wav"));
    }

    // One column grid, header included: a header that sits over its own column's
    // content is the whole point of a table, and a row that negotiates its widths
    // against its own content puts one row's length under another row's role.
    [AvaloniaFact]
    public void EveryRowCarriesTheSameColumnsAsTheHeader()
    {
        var table = Build(SheetTable(Disc(1)));
        var columns = Rows(table).Select(ColumnWidths).ToList();

        Assert.All(columns, widths => Assert.Equal(columns[0], widths));
        // Every width is the table's, not the row's, and every one of them is a
        // number: a column that stretches is a column the row gets to argue
        // about against its own content.
        Assert.All(
            columns[0],
            width => Assert.Equal(GridUnitType.Pixel, width.GridUnitType));
    }

    // The whole row has to fit the pane. Seven columns at a width the pane never
    // had is what ran the length and the row's actions off its right edge.
    [Theory]
    [InlineData(400)]
    [InlineData(ImportMappingColumns.MinimumWidth)]
    [InlineData(700)]
    [InlineData(900)]
    [InlineData(ImportMappingColumns.IdealWidth)]
    [InlineData(1600)]
    public void TheColumnsAddUpToTheTable(double paneWidth)
    {
        var columns = ImportMappingColumns.Resolve(paneWidth);

        var row = columns.Source + columns.Role + columns.Title + columns.Artist
            + ImportMappingColumns.Position + ImportMappingColumns.Length
            + ImportMappingColumns.Actions + ImportMappingColumns.Spacing * 6;

        // Under its minimum the table stops shrinking; the pane scrolls it
        // sideways from there rather than squeezing a column out of the row.
        Assert.Equal(Math.Max(paneWidth, ImportMappingColumns.MinimumWidth), row, 6);
    }

    // Wide enough and every column has the width it asks for, with the surplus
    // going to the file names and nowhere else — the layout a wide window has
    // always drawn, unchanged. The same four numbers the macOS table resolves.
    [Fact]
    public void TheSurplusAboveTheIdealWidthIsAllTheSource()
    {
        var columns = ImportMappingColumns.Resolve(ImportMappingColumns.IdealWidth + 300);

        Assert.Equal(118, columns.Role);
        Assert.Equal(220, columns.Title);
        Assert.Equal(180, columns.Artist);
        Assert.Equal(540, columns.Source);
    }

    // Narrowing takes from all four at once. A column that kept its width while
    // its neighbour collapsed would be the same bug in miniature: the row still
    // fits, and one cell has stopped saying anything.
    [Fact]
    public void NarrowingTakesFromEveryColumnThatHasGive()
    {
        var wide = ImportMappingColumns.Resolve(ImportMappingColumns.IdealWidth);
        var middle = ImportMappingColumns.Resolve(
            (ImportMappingColumns.IdealWidth + ImportMappingColumns.MinimumWidth) / 2);
        var narrow = ImportMappingColumns.Resolve(ImportMappingColumns.MinimumWidth);

        Assert.True(middle.Source < wide.Source);
        Assert.True(middle.Role < wide.Role);
        Assert.True(middle.Title < wide.Title);
        Assert.True(middle.Artist < wide.Artist);
        Assert.True(narrow.Source < middle.Source);
        Assert.True(narrow.Role < middle.Role);
        Assert.True(narrow.Title < middle.Title);
        Assert.True(narrow.Artist < middle.Artist);
    }

    // Cue filenames are arbitrary, so the header carries the assignment: every
    // disc the release has, plus taking the sheet out of the tracklist.
    [AvaloniaFact]
    public void TheDiscControlOffersEveryDiscAndIgnored()
    {
        var assigned = new List<(string Sheet, BridgeSheetDisc Disc)>();
        var table = Build(SheetTable(Disc(1)), setSheetDisc: (sheet, disc) => assigned.Add((sheet, disc)));
        var menu = DiscMenu(Rows(table)[1]);

        Assert.Equal(
            new[]
            {
                Loc.Core("ui.import.sheet.disc", "number", 1L),
                Loc.Core("ui.import.sheet.disc", "number", 2L),
                Loc.Core("ui.import.sheet.ignored"),
            },
            menu.Select(item => item.Header as string).ToArray());

        menu[1].RaiseEvent(new RoutedEventArgs(MenuItem.ClickEvent));
        menu[2].RaiseEvent(new RoutedEventArgs(MenuItem.ClickEvent));

        Assert.Equal(
            new BridgeSheetDisc[] { new BridgeSheetDisc.Disc(2), new BridgeSheetDisc.Ignored() },
            assigned.Select(entry => entry.Disc).ToArray());
        Assert.All(assigned, entry => Assert.Equal(SheetId, entry.Sheet));
    }

    // The sheet's own text is what says which disc it holds, so its name opens
    // in the document viewer from the header.
    [AvaloniaFact]
    public void TheSheetNameOpensTheSheetInTheDocumentViewer()
    {
        var opened = new List<(string Name, string Path)>();
        var table = Build(SheetTable(Disc(1)), openDocument: (name, path) => opened.Add((name, path)));

        NameButton(Rows(table)[1]).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.Equal(new[] { (SheetId, "/folder/disc.cue") }, opened);
    }

    // Auditioning a sheet entry plays the container it is carved out of — the
    // only file on disk there is to play.
    [AvaloniaFact]
    public void AuditioningAnEntryPlaysItsContainer()
    {
        var played = new List<string>();
        var table = Build(SheetTable(Disc(1)), preview: played.Add);

        PlayButton(Rows(table)[2]).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.Equal(new[] { ContainerPath }, played);
    }

    // The folder's images are one row: a tile per image, the one that leads the
    // release marked as such, and clicking a tile opens the lightbox over the
    // whole gallery at the image that was clicked.
    [AvaloniaFact]
    public void TheImagesAreOneGalleryRowThatOpensTheLightbox()
    {
        var opened = new List<(int Count, string Path)>();
        var table = Build(
            new BridgeMappingTable(
                new BridgeMappingRow[] { new BridgeMappingRow.Images(Images()) },
                Reconciliation: null),
            openImages: (images, path) => opened.Add((images.Count, path)));

        var tiles = Rows(table)[1].GetLogicalDescendants().OfType<Button>().ToList();
        Assert.Equal(2, tiles.Count);
        Assert.Contains(
            table.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == Loc.Core("ui.import.becomes.cover"));
        // What becomes of them is stated once for the group, not once per tile.
        Assert.Equal(
            1,
            table.GetLogicalDescendants().OfType<TextBlock>()
                .Count(text => text.Text == Loc.Core("ui.import.becomes.kept")));

        tiles[1].RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.Equal(new[] { (2, "/folder/back.jpg") }, opened);
    }

    // ── Reading the table ────────────────────────────────────────────────────

    // The commit bar states what committing writes and what is still unnamed;
    // a track the release names with no audio behind it writes nothing.
    [Fact]
    public void TheCommitCountsCoverTheRowsThatWriteTracks()
    {
        var table = new BridgeMappingTable(
            new BridgeMappingRow[]
            {
                Unit(FileSource("01.flac"), TrackBecomes("t0", "Track Title 1", Standalone("01.flac"))),
                Unit(FileSource("02.flac"), TrackBecomes("t1", "   ", Standalone("02.flac"))),
                Unit(new BridgeMappingSource.Missing(), TrackBecomes("t2", "Track Title 3", file: null)),
            },
            new BridgeSlotReconciliation.MoreTracks(2, 3));

        Assert.Equal(2, table.WillWriteCount());
        Assert.Equal(1, table.UnansweredCount());
    }

    // A row with nothing behind it is offered the folder's audio by name, which
    // is every unit the table already carries.
    [Fact]
    public void TheAudioChoicesAreTheUnitsThatCarryAudio()
    {
        var table = new BridgeMappingTable(
            new BridgeMappingRow[]
            {
                Unit(FileSource("01.flac"), TrackBecomes("t0", "Track Title 1", Standalone("01.flac"))),
                Unit(new BridgeMappingSource.Missing(), TrackBecomes("t1", "Track Title 2", file: null)),
            },
            new BridgeSlotReconciliation.MoreTracks(1, 2));

        var choices = table.AudioChoices();

        Assert.Single(choices);
        Assert.Equal(Standalone("01.flac"), choices[0].Audio);
        Assert.StartsWith("01.flac", choices[0].Label, System.StringComparison.Ordinal);
    }

    // The tally's message takes a different pair of numbers per variant, so the
    // arguments ride with core's key rather than being assembled per call site.
    [Fact]
    public void TheReconciliationArgumentsMatchTheVariant()
    {
        var agrees = MappingTableReading.ReconciliationArgs(new BridgeSlotReconciliation.Agrees(12));
        Assert.Equal(12L, agrees["count"]);
        Assert.Single(agrees);

        var more = MappingTableReading.ReconciliationArgs(new BridgeSlotReconciliation.MoreFiles(13, 12));
        Assert.Equal(13L, more["files"]);
        Assert.Equal(12L, more["tracks"]);
        Assert.False(more.ContainsKey("count"));
    }

    // ── Fixtures ─────────────────────────────────────────────────────────────

    // A folder whose only row is a track sheet on nothing, with the two entries
    // it prints. Nothing is picked, so both entries await one.
    private static BridgeMappingTable SheetTable(BridgeSheetDisc assignment) => new(
        new BridgeMappingRow[]
        {
            new BridgeMappingRow.Sheet(
                new BridgeSheetGroup(
                    SheetId: SheetId,
                    Name: SheetId,
                    LocalPath: "/folder/disc.cue",
                    Bound: new BridgeSheetBound.Unresolved(new[] { "disc.wav" }),
                    Assignment: assignment,
                    DiscOptions: new uint[] { 1, 2 }),
                new[] { Entry(0), Entry(1) }),
        },
        Reconciliation: null);

    private static BridgeSheetDisc Disc(uint number) => new BridgeSheetDisc.Disc(number);

    private static BridgeMappingUnit Entry(uint index) => new(
        new BridgeMappingSource.SheetEntry(new BridgeMappingEntry(
            SheetId: SheetId,
            Index: index,
            Number: index + 1,
            Title: $"Sheet Track {index + 1}",
            DurationMs: null,
            ContainerId: "disc.flac",
            ContainerName: "disc.flac",
            ContainerLocalPath: ContainerPath)),
        new BridgeMappingBecomes.AwaitingPick());

    private static BridgeMappingImage[] Images() => new[]
    {
        new BridgeMappingImage(
            FileId: "front.jpg",
            Name: "front.jpg",
            Size: 2048,
            LocalPath: "/folder/front.jpg",
            IsCover: true),
        new BridgeMappingImage(
            FileId: "back.jpg",
            Name: "back.jpg",
            Size: 1024,
            LocalPath: "/folder/back.jpg",
            IsCover: false),
    };

    private static BridgeMappingRow Unit(BridgeMappingSource source, BridgeMappingBecomes becomes) =>
        new BridgeMappingRow.Unit(new BridgeMappingUnit(source, becomes));

    private static BridgeMappingSource FileSource(string fileId) =>
        new BridgeMappingSource.File(new BridgeMappingFile(
            FileId: fileId,
            Name: fileId,
            Size: 1024,
            LocalPath: $"/folder/{fileId}",
            ProbedDurationMs: null,
            Role: BridgeMappingRole.Audio,
            Alternatives: System.Array.Empty<BridgeFileRoleChoice>(),
            RoleChoice: null));

    private static BridgeMappingBecomes TrackBecomes(string id, string title, BridgeAudioFile? file) =>
        new BridgeMappingBecomes.Track(
            new BridgeRawTrackEdit(id, title, string.Empty, 1, null, file),
            SourcePosition: null,
            SourceDurationMs: null);

    private static BridgeAudioFile Standalone(string fileId) => new BridgeAudioFile.Standalone(fileId);

    // ── Building and reaching into the control ───────────────────────────────

    private static Control Build(
        BridgeMappingTable table,
        System.Action<string, BridgeSheetDisc>? setSheetDisc = null,
        System.Action<string, string>? openDocument = null,
        System.Action<string>? preview = null,
        System.Action<IReadOnlyList<BridgeMappingImage>, string>? openImages = null) =>
        new ImportMappingTable(
            table,
            _ => Task.FromResult(new List<ImportSheetBindingOption>()),
            () => null,
            (_, _) => { },
            new ImportMappingActions(
                SetRole: (_, _) => { },
                BindSheet: (_, _) => { },
                SetSheetDisc: setSheetDisc ?? ((_, _) => { }),
                OpenDocument: openDocument ?? ((_, _) => { }),
                OpenImages: openImages ?? ((_, _) => { }),
                Preview: preview ?? (_ => { }),
                StopPreview: () => { },
                EditTrack: _ => { },
                ChooseFile: (_, _) => { },
                Drop: _ => { },
                Exclude: _ => { })).Build();

    /// <summary>The table's children: the column header, then one per rendered
    /// row — a sheet's own row and each of its entries. The table hangs inside
    /// the scroller that carries it sideways in a pane too narrow for its
    /// columns.</summary>
    private static IReadOnlyList<Control> Rows(Control table) =>
        ((StackPanel)((ScrollViewer)table).Content!).Children.OfType<Control>().ToList();

    /// <summary>A row's left half, which is the first cell of its grid.</summary>
    private static Control SourceCell(Control row) =>
        (Control)((Grid)((Border)row).Child!).Children[0];

    /// <summary>The columns a row lays its cells out over — the header's own
    /// grid included, which is a bare grid rather than a hosted row.</summary>
    private static IReadOnlyList<GridLength> ColumnWidths(Control row) =>
        (row is Border border ? (Grid)border.Child! : (Grid)row)
            .ColumnDefinitions.Select(column => column.Width).ToList();

    private static IReadOnlyList<MenuItem> DiscMenu(Control row) =>
        row.GetLogicalDescendants()
            .OfType<Button>()
            .Select(button => button.Flyout)
            .OfType<MenuFlyout>()
            .SelectMany(flyout => flyout.ItemsSource!.OfType<MenuItem>())
            .ToList();

    private static Button NameButton(Control row) =>
        row.GetLogicalDescendants().OfType<Button>().First(button => button.Content is TextBlock);

    private static Button PlayButton(Control row) =>
        row.GetLogicalDescendants()
            .OfType<Button>()
            .First(button => Equals(button.Content, Loc.Core("ui.import.slots.play")));
}
