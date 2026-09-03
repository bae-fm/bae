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
    private static readonly BridgeAudioFormat SourceAudio = new(
        Codec: "FLAC",
        SampleRateHz: 44_100,
        BitsPerSample: 16,
        BitrateKbps: null,
        Channels: 2);

    [Fact]
    public void ArtistFillFollowsTheSelectedRowDownward()
    {
        var selection = new ArtistFillSelection(sourceIndex: 1);

        selection.ExtendTo(3);

        Assert.Equal(new[] { 1, 2, 3 }, selection.Indexes());
    }

    // ── The table, rendered ──────────────────────────────────────────────────

    // Tracks and files are two questions, so they are two sections. A file the
    // release does not name is not a track with an empty title — it is a file,
    // and it is listed as one, with the job it has and no sentence saying it is
    // kept.
    [AvaloniaFact]
    public void TracksAndFilesAreListedApart()
    {
        var table = Build(new BridgeMappingTable(
            Array.Empty<BridgeMappingImage>(),
            new BridgeMappingTrackSection[]
            {
                FlatSection(Mapping(
                    FileSource("01.flac"),
                    TrackBecomes("t1", "Track One", Standalone("01.flac")))),
            },
            new BridgeMappingFileRow[]
            {
                new BridgeMappingFileRow.File(MappingFile("rip.log")),
            },
            Reconciliation: null));

        // The header plus the one track; the log is not among them.
        Assert.Equal(2, Rows(table).Count);
        var files = FileRows(table);
        Assert.Equal(2, files.Count);
        Assert.Contains(
            files[1].GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == "rip.log");
        Assert.Contains(
            table.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == Loc.Core("ui.import.mapping.tracks_title"));
        Assert.Contains(
            table.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == Loc.Core("ui.import.mapping.files_title"));
    }

    [AvaloniaFact]
    public void TrackAndFileHeadersNameTheirOwnMeasurements()
    {
        var table = Build(new BridgeMappingTable(
            Array.Empty<BridgeMappingImage>(),
            new[]
            {
                FlatSection(Mapping(
                    FileSource("01.flac"),
                    TrackBecomes("t1", "Track One", Standalone("01.flac")))),
            },
            new BridgeMappingFileRow[]
            {
                new BridgeMappingFileRow.File(MappingFile("notes.txt")),
            },
            Reconciliation: null));

        Assert.Contains(
            Rows(table)[0].GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == Loc.Core("ui.import.slots.column.length")
                .ToUpper(System.Globalization.CultureInfo.CurrentUICulture));
        Assert.Contains(
            FileRows(table)[0].GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == Loc.Chrome("storage.column.size")
                .ToUpper(System.Globalization.CultureInfo.CurrentUICulture));
    }

    // The sheet a disc ID was computed from wears the chip on its own row, and
    // that row hovers to the sentence naming the ID. Nothing else does.
    [AvaloniaFact]
    public void TheSheetTheDiscIdCameFromSaysSo()
    {
        var evidence = new[]
        {
            new BridgeFileEvidence(
                BridgeEvidenceSignal.DiscId,
                Value: "XwqRcz4RhAqRTfhE5nRxRKF4iFY-",
                FileId: SheetId),
        };
        var table = Build(SheetTable(Disc(1)), evidence: evidence);

        var chips = table
            .GetLogicalDescendants()
            .OfType<TextBlock>()
            .Where(text => text.Text == Loc.Chrome("signal.kind.disc_id"))
            .ToList();

        var chip = Assert.Single(chips);
        Assert.Equal(
            Loc.Core(
                "core.import.evidence.disc_id_from_file",
                "value",
                "XwqRcz4RhAqRTfhE5nRxRKF4iFY-"),
            ToolTip.GetTip((Control)chip.Parent!));
    }

    [AvaloniaFact]
    public void CoreSuppliedSideHeadingIsRenderedAboveItsTracks()
    {
        var table = Build(new BridgeMappingTable(
            Array.Empty<BridgeMappingImage>(),
            new[]
            {
                new BridgeMappingTrackSection(
                    new BridgeTrackSide.Sided("A"),
                    "core.track.side",
                    new BridgeMappingTrackSectionContent.Tracks(new[]
                    {
                        Mapping(
                            FileSource("01.flac"),
                            TrackBecomes("t1", "Track One", Standalone("01.flac"))),
                    })),
            },
            Array.Empty<BridgeMappingFileRow>(),
            Reconciliation: null));

        Assert.Contains(
            table.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == Loc.Core("core.track.side", "letter", "A")
                .ToUpper(System.Globalization.CultureInfo.CurrentUICulture));
    }

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

    // The whole row has to fit the pane. Columns resolved at a width the pane
    // never had is what ran the last of them off its right edge.
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

        var row = columns.Title + columns.Artist + columns.Source
            + ImportMappingColumns.Position + ImportMappingColumns.Length
            + (ImportMappingColumns.Spacing * 4);

        // Under its minimum the table stops shrinking; the pane scrolls it
        // sideways from there rather than squeezing a column out of the row.
        Assert.Equal(Math.Max(paneWidth, ImportMappingColumns.MinimumWidth), row, 6);
    }

    // Wide enough and every column has the width it asks for, with the surplus
    // going to the file names and nowhere else. The same three numbers the
    // macOS table resolves.
    [Fact]
    public void TheSurplusAboveTheIdealWidthIsAllTheSource()
    {
        var columns = ImportMappingColumns.Resolve(ImportMappingColumns.IdealWidth + 300);

        Assert.Equal(220, columns.Title);
        Assert.Equal(180, columns.Artist);
        Assert.Equal(560, columns.Source);
    }

    // Narrowing takes from all three at once. A column that kept its width while
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
        Assert.True(middle.Title < wide.Title);
        Assert.True(middle.Artist < wide.Artist);
        Assert.True(narrow.Source < middle.Source);
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

    // Auditioning a sheet entry plays only that entry's window of its container.
    [AvaloniaFact]
    public void AuditioningAnEntryPlaysItsExactSourceWindow()
    {
        var played = new List<BridgePreviewTarget>();
        var table = Build(SheetTable(Disc(1)), preview: played.Add);

        PlayButton(Rows(table)[2]).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.Equal(new[] { EntryTarget(0) }, played);
    }

    // Two sheet entries share one container path, but they are different
    // preview identities. Playing the second must not turn the first row into a
    // stop button or make clicking it stop the second.
    [AvaloniaFact]
    public void SheetEntryPreviewIdentityIncludesItsSampleWindow()
    {
        var played = new List<BridgePreviewTarget>();
        var stopped = false;
        var table = Build(
            SheetTable(Disc(1)),
            preview: played.Add,
            previewingTarget: EntryTarget(1),
            stopPreview: () => stopped = true);
        var rows = Rows(table);

        Assert.Equal(
            Loc.Core("ui.import.slots.play"),
            PlayButton(rows[2]).Content);
        Assert.Contains(
            rows[3].GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, Loc.Core("ui.import.slots.stop")));

        PlayButton(rows[2]).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.False(stopped);
        Assert.Equal(new[] { EntryTarget(0) }, played);
    }

    // ── Reading the table ────────────────────────────────────────────────────

    // The commit bar states what committing writes and what is still unnamed;
    // a track the release names with no audio behind it writes nothing.
    [Fact]
    public void TheCommitCountsCoverTheRowsThatWriteTracks()
    {
        var table = new BridgeMappingTable(
            Array.Empty<BridgeMappingImage>(),
            new BridgeMappingTrackSection[]
            {
                FlatSection(
                    Mapping(FileSource("01.flac"), TrackBecomes("t0", "Track Title 1", Standalone("01.flac"))),
                    Mapping(FileSource("02.flac"), TrackBecomes("t1", "   ", Standalone("02.flac"))),
                    Mapping(new BridgeMappingSource.Missing(), TrackBecomes("t2", "Track Title 3", file: null))),
            },
            Array.Empty<BridgeMappingFileRow>(),
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
            Array.Empty<BridgeMappingImage>(),
            new BridgeMappingTrackSection[]
            {
                FlatSection(
                    Mapping(FileSource("01.flac"), TrackBecomes("t0", "Track Title 1", Standalone("01.flac"))),
                    Mapping(new BridgeMappingSource.Missing(), TrackBecomes("t1", "Track Title 2", file: null))),
            },
            Array.Empty<BridgeMappingFileRow>(),
            new BridgeSlotReconciliation.MoreTracks(1, 2));

        var choices = table.AudioChoices();

        Assert.Single(choices);
        Assert.Equal(Standalone("01.flac"), choices[0].Audio);
        Assert.StartsWith("01.flac", choices[0].Label, System.StringComparison.Ordinal);
    }

    [AvaloniaFact]
    public void ProbedDurationRendersWhenMetadataNamesNoDuration()
    {
        var source = new BridgeMappingSource.File(new BridgeMappingFile(
            FileId: "01.flac",
            Name: "01.flac",
            Size: 1024,
            LocalPath: "/folder/01.flac",
            PreviewTarget: new BridgePreviewTarget("/folder/01.flac", 0, null),
            DurationMs: 180_000,
            AudioFormat: SourceAudio,
            Role: BridgeMappingRole.Audio,
            Alternatives: Array.Empty<BridgeFileRoleChoice>(),
            RoleChoice: null));
        var table = Build(new BridgeMappingTable(
            Array.Empty<BridgeMappingImage>(),
            new[]
            {
                FlatSection(Mapping(
                    source,
                    TrackBecomes(
                        "t0",
                        "Track Title",
                        Standalone("01.flac")),
                    durationMs: 180_000)),
            },
            Array.Empty<BridgeMappingFileRow>(),
            Reconciliation: null));

        Assert.Contains(
            table.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == "3:00");
    }

    [AvaloniaFact]
    public void ProbedDurationRendersBeforeMetadataIsChosen()
    {
        var source = new BridgeMappingSource.File(new BridgeMappingFile(
            FileId: "01.flac",
            Name: "01.flac",
            Size: 1024,
            LocalPath: "/folder/01.flac",
            PreviewTarget: new BridgePreviewTarget("/folder/01.flac", 0, null),
            DurationMs: 180_000,
            AudioFormat: SourceAudio,
            Role: BridgeMappingRole.Audio,
            Alternatives: Array.Empty<BridgeFileRoleChoice>(),
            RoleChoice: null));
        var table = Build(new BridgeMappingTable(
            Array.Empty<BridgeMappingImage>(),
            new[]
            {
                FlatSection(Mapping(
                    source,
                    new BridgeMappingBecomes.AwaitingPick(),
                    durationMs: 180_000)),
            },
            Array.Empty<BridgeMappingFileRow>(),
            Reconciliation: null));

        Assert.Contains(
            table.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == "3:00");
    }

    // The tally's message takes a different pair of numbers per variant, so the
    // arguments ride with core's key rather than being assembled per call site.
    [Fact]
    public void TheReconciliationArgumentsMatchTheVariant()
    {
        var more = MappingTableReading.ReconciliationArgs(new BridgeSlotReconciliation.MoreFiles(13, 12));
        Assert.Equal(13L, more["files"]);
        Assert.Equal(12L, more["tracks"]);
        Assert.False(more.ContainsKey("count"));
    }

    // Two sides that account for the same rows say nothing the table is not
    // already showing, so they draw no line at all.
    [Fact]
    public void AnAgreementDrawsNoReconciliationLine()
    {
        Assert.Null(MappingTableReading.ReconciliationLine(new BridgeSlotReconciliation.Agrees(12)));
        Assert.NotNull(
            MappingTableReading.ReconciliationLine(new BridgeSlotReconciliation.MoreFiles(13, 12)));
    }

    // ── Fixtures ─────────────────────────────────────────────────────────────

    // A folder whose only row is a track sheet on nothing, with the two entries
    // it prints. Nothing is picked, so both entries await one.
    private static BridgeMappingTable SheetTable(BridgeSheetDisc assignment) => new(
        Array.Empty<BridgeMappingImage>(),
        new BridgeMappingTrackSection[]
        {
            new BridgeMappingTrackSection(
                new BridgeTrackSide.Flat(),
                HeaderKey: null,
                new BridgeMappingTrackSectionContent.Sheet(
                    new BridgeSheetGroup(
                        SheetId: SheetId,
                        Name: SheetId,
                        Size: 2048,
                        LocalPath: "/folder/disc.cue",
                        Bound: new BridgeSheetBound.Unresolved(new[] { "disc.wav" }),
                        Assignment: assignment,
                        DiscOptions: new uint[] { 1, 2 }),
                    new[] { Entry(0), Entry(1) })),
        },
        Array.Empty<BridgeMappingFileRow>(),
        Reconciliation: null);

    private static BridgeSheetDisc Disc(uint number) => new BridgeSheetDisc.Disc(number);

    private static BridgeTrackMapping Entry(uint index) => new(
        new BridgeMappingSource.SheetEntry(new BridgeMappingEntry(
            SheetId: SheetId,
            Index: index,
            Number: index + 1,
            Title: $"Sheet Track {index + 1}",
            DurationMs: null,
            ContainerId: "disc.flac",
            ContainerName: "disc.flac",
            ContainerLocalPath: ContainerPath,
            PreviewTarget: EntryTarget(index),
            AudioFormat: SourceAudio)),
        new BridgeMappingBecomes.AwaitingPick(),
        DurationMs: null);

    private static BridgeTrackMapping Mapping(
        BridgeMappingSource source,
        BridgeMappingBecomes becomes,
        ulong? durationMs = null) => new(source, becomes, durationMs);

    private static BridgeMappingTrackSection FlatSection(
        params BridgeTrackMapping[] mappings) => new(
            new BridgeTrackSide.Flat(),
            HeaderKey: null,
            new BridgeMappingTrackSectionContent.Tracks(mappings));

    private static BridgeMappingSource FileSource(string fileId) =>
        new BridgeMappingSource.File(MappingFile(fileId));

    private static BridgeMappingFile MappingFile(string fileId) => new(
            FileId: fileId,
            Name: fileId,
            Size: 1024,
            LocalPath: $"/folder/{fileId}",
            PreviewTarget: fileId.EndsWith(".flac", StringComparison.Ordinal)
                ? new BridgePreviewTarget($"/folder/{fileId}", 0, null)
                : null,
            DurationMs: null,
            AudioFormat: fileId.EndsWith(".flac", StringComparison.Ordinal)
                ? SourceAudio
                : null,
            Role: BridgeMappingRole.Audio,
            Alternatives: System.Array.Empty<BridgeFileRoleChoice>(),
            RoleChoice: null);

    private static BridgeMappingBecomes TrackBecomes(
        string id,
        string title,
        BridgeAudioFile? file) =>
        new BridgeMappingBecomes.Track(
            new BridgeRawTrackEdit(
                id,
                title,
                new BridgeTrackArtistAssignments.AlbumArtists(),
                1,
                null,
                file),
            Position: "1",
            NamedBySource: true);

    private static BridgeAudioFile Standalone(string fileId) => new BridgeAudioFile.Standalone(fileId);

    private static BridgePreviewTarget EntryTarget(uint index) => new(
        ContainerPath,
        StartSample: index * 44_100,
        EndSample: (index + 1) * 44_100);

    // ── Building and reaching into the control ───────────────────────────────

    private static Control Build(
        BridgeMappingTable table,
        System.Action<string, BridgeSheetDisc>? setSheetDisc = null,
        System.Action<string, string>? openDocument = null,
        System.Action<BridgePreviewTarget>? preview = null,
        BridgePreviewTarget? previewingTarget = null,
        System.Action? stopPreview = null,
        BridgeFileEvidence[]? evidence = null) =>
        new ImportMappingTable(
            table,
            _ => Task.FromResult(new List<ImportSheetBindingOption>()),
            () => previewingTarget,
            new LibraryService(),
            new ImportMappingActions(
                SetRole: (_, _) => { },
                BindSheet: (_, _) => { },
                SetSheetDisc: setSheetDisc ?? ((_, _) => { }),
                OpenDocument: openDocument ?? ((_, _) => { }),
                OpenImages: (_, _) => { },
                Preview: preview ?? (_ => { }),
                StopPreview: stopPreview ?? (() => { }),
                EditTrack: _ => { },
                SetTrackArtists: (_, _) => { },
                ChooseFile: (_, _) => { },
                Drop: _ => { },
                Exclude: _ => { }),
            evidence: evidence).Build();

    /// <summary>The Tracks section's children: the column header, then one per
    /// rendered row — a sheet's own row and each of its slices. Each section is
    /// its heading over a scroller that carries the rows sideways in a pane too
    /// narrow for the columns.</summary>
    private static IReadOnlyList<Control> Rows(Control table) => SectionRows(table, 0);

    /// <summary>The Files section's children. Absent, and so empty, for a
    /// folder whose every row becomes a track.</summary>
    private static IReadOnlyList<Control> FileRows(Control table) =>
        ((StackPanel)table).Children.Count > 1
            ? SectionRows(table, 1)
            : Array.Empty<Control>();

    private static IReadOnlyList<Control> SectionRows(Control table, int index)
    {
        var section = (StackPanel)((StackPanel)table).Children[index];
        var scroller = (ScrollViewer)section.Children[1];
        var rows = scroller.Content is Grid layers
            ? (StackPanel)layers.Children[0]
            : (StackPanel)scroller.Content!;
        return rows.Children.OfType<Control>().ToList();
    }

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
