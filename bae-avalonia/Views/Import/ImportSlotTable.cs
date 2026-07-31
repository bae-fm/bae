using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The mapping pane's third zone: one row per track slot, with the
/// reconciliation line above it.
///
/// A row is a file and the track it becomes at once — the position the source
/// gives it, the audio in mono, a glyph carrying the pairing state, the two
/// editable fields, and both durations. The pair of durations is the only thing
/// that catches a pairing which is complete but wrong, so it is stated on every
/// row and marked where the two disagree; the mark is a note, and neither it nor
/// the tally above ever disables anything.
/// </summary>
internal sealed class ImportSlotTable
{
    private readonly MappingPaneModel _model;
    private readonly IReadOnlyList<BridgeSlotFile> _audio;
    private readonly Action<int, string> _onTitle;
    private readonly Action<int, string> _onArtist;
    private readonly Action<int> _onExclude;
    private readonly Action<int> _onDrop;
    private readonly Action<int, BridgeSlotFile> _onChooseFile;
    private readonly Action<int> _onPlay;
    private readonly Action _onStop;
    private readonly Func<string?> _previewingPath;

    // The row hosts, in row order, so the accent on the playing row moves
    // without rebuilding the table under the fields the user is typing in.
    private readonly List<Border> _rowHosts = new();

    internal ImportSlotTable(
        MappingPaneModel model,
        IReadOnlyList<BridgeSlotFile> audio,
        Action<int, string> onTitle,
        Action<int, string> onArtist,
        Action<int> onExclude,
        Action<int> onDrop,
        Action<int, BridgeSlotFile> onChooseFile,
        Action<int> onPlay,
        Action onStop,
        Func<string?> previewingPath)
    {
        _model = model;
        _audio = audio;
        _onTitle = onTitle;
        _onArtist = onArtist;
        _onExclude = onExclude;
        _onDrop = onDrop;
        _onChooseFile = onChooseFile;
        _onPlay = onPlay;
        _onStop = onStop;
        _previewingPath = previewingPath;
    }

    internal Control Build()
    {
        _rowHosts.Clear();
        var column = new StackPanel { Spacing = 0 };
        column.Children.Add(ImportPaneUi.ZoneTitle(Loc.Core("ui.import.slots.title"), ReconciliationLine()));
        column.Children.Add(HeaderRow());

        for (var index = 0; index < _model.Rows.Count; index++)
        {
            column.Children.Add(Row(index, _model.Rows[index]));
        }
        ApplyPreviewAccent();
        return column;
    }

    /// <summary>Move the accent onto whichever row the preview transport is
    /// playing. Called on every preview event; it touches only the row
    /// backgrounds, so a field mid-edit keeps its focus and its caret.</summary>
    internal void ApplyPreviewAccent()
    {
        var playing = _previewingPath();
        for (var index = 0; index < _rowHosts.Count && index < _model.Rows.Count; index++)
        {
            if (_model.IsPlaying(index, playing))
            {
                _rowHosts[index][!Border.BackgroundProperty] =
                    new DynamicResourceExtension("BaeSelectionTintBrush");
            }
            else
            {
                _rowHosts[index].Background = Brushes.Transparent;
            }
        }
    }

    // The tally above the table, as core computed it: how many files the folder
    // offers against how many tracks the source names, and which way they
    // disagree. A statement, never a warning that gates anything. Absent for an
    // import with no release mapped onto it — there is no tracklist to
    // reconcile the folder against.
    private string? ReconciliationLine() =>
        _model.Reconciliation is { } reconciliation
            ? Loc.Core(
                BaeBridgeMethods.BridgeSlotReconciliationKey(Bridge(reconciliation)),
                MappingPaneModel.ReconciliationArgs(reconciliation))
            : null;

    private static BridgeSlotReconciliation Bridge(MappingReconciliation reconciliation) => reconciliation.Kind switch
    {
        MappingReconciliationKind.Agrees => new BridgeSlotReconciliation.Agrees(reconciliation.Count),
        MappingReconciliationKind.MoreFiles =>
            new BridgeSlotReconciliation.MoreFiles(reconciliation.Files, reconciliation.Tracks),
        _ => new BridgeSlotReconciliation.MoreTracks(reconciliation.Files, reconciliation.Tracks),
    };

    private static Grid Grid() => new()
    {
        ColumnDefinitions = new ColumnDefinitions("42,*,18,*,*,74,Auto"),
        ColumnSpacing = 8,
    };

    private static Control HeaderRow()
    {
        var grid = Grid();
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Chrome("edit.tracks.col_number"), 0));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.slots.column.file"), 1));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Chrome("edit.tracks.col_title"), 3));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Chrome("edit.tracks.col_artist"), 4));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.slots.column.length"), 5));
        grid.Margin = new Thickness(0, 0, 0, 4);
        return grid;
    }

    private Control Row(int index, MappingSlotRow row)
    {
        var grid = Grid();

        var position = ImportPaneUi.Cell(row.Position, secondary: true);
        Avalonia.Controls.Grid.SetColumn(position, 0);
        grid.Children.Add(position);

        var file = FileCell(row);
        Avalonia.Controls.Grid.SetColumn(file, 1);
        grid.Children.Add(file);

        var link = new TextBlock
        {
            Text = MappingPaneModel.LinkGlyph(row.Kind, row.Span),
            FontSize = 13,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        link[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(
            row.Kind == MappingSlotKind.Paired ? "BaeAccentBrush" : "BaeTextSecondaryBrush");
        Avalonia.Controls.Grid.SetColumn(link, 2);
        grid.Children.Add(link);

        var title = new TextBox
        {
            Text = row.Title,
            FontSize = 12.5,
            Watermark = Loc.Core("ui.import.slots.untitled"),
            VerticalAlignment = VerticalAlignment.Center,
        };
        title.TextChanged += (_, _) => _onTitle(index, title.Text ?? string.Empty);
        Avalonia.Controls.Grid.SetColumn(title, 3);
        grid.Children.Add(title);

        var artist = new TextBox
        {
            Text = row.ArtistText,
            FontSize = 12.5,
            Watermark = Loc.Chrome("edit.tracks.artist_placeholder"),
            VerticalAlignment = VerticalAlignment.Center,
        };
        artist.TextChanged += (_, _) => _onArtist(index, artist.Text ?? string.Empty);
        Avalonia.Controls.Grid.SetColumn(artist, 4);
        grid.Children.Add(artist);

        var lengths = Lengths(row);
        Avalonia.Controls.Grid.SetColumn(lengths, 5);
        grid.Children.Add(lengths);

        var actions = Actions(index, row);
        Avalonia.Controls.Grid.SetColumn(actions, 6);
        grid.Children.Add(actions);

        var host = new Border { Padding = new Thickness(4, 4), CornerRadius = new CornerRadius(6), Child = grid };
        _rowHosts.Add(host);
        return host;
    }

    // The audio behind the row, or the dashed placeholder a row the source
    // names but nothing backs shows in its place.
    private static Control FileCell(MappingSlotRow row)
    {
        if (row.FileName is not { } name)
        {
            var empty = ImportPaneUi.Cell(
                row.Kind == MappingSlotKind.TrackOnly ? $"╌ {Loc.Core("ui.import.slots.no_file")}" : string.Empty,
                secondary: true);
            empty.FontFamily = new FontFamily("monospace");
            empty.FontSize = 12;
            return empty;
        }
        return ImportPaneUi.FileName(null, name, checked((long)row.FileSize));
    }

    // Both lengths, the probed file's leading and the source's under it. A
    // missing number is an em dash, never a zero, and a pair that disagrees by
    // more than a moment says so.
    private static Control Lengths(MappingSlotRow row)
    {
        var probed = new TextBlock
        {
            Text = Clock(row.ProbedDurationMs),
            FontSize = 12,
            FontFamily = new FontFamily("monospace"),
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        var source = new TextBlock
        {
            Text = Clock(row.SourceDurationMs),
            FontSize = 11,
            FontFamily = new FontFamily("monospace"),
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        // Core's rule, asked here rather than carried on the row: re-pointing a
        // row at a different file gives it two new lengths, and an answer
        // settled when the mapping was computed would still describe the
        // pairing it replaced. It marks the pair; it disables nothing.
        var disagree = BaeBridgeMethods.BridgeLengthsDisagree(row.ProbedDurationMs, row.SourceDurationMs);
        probed[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension(disagree ? "BaeWarningBrush" : "BaeTextPrimaryBrush");
        source[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension(disagree ? "BaeWarningBrush" : "BaeTextSecondaryBrush");

        var column = new StackPanel { Spacing = 0, VerticalAlignment = VerticalAlignment.Center };
        column.Children.Add(probed);
        column.Children.Add(source);
        if (disagree)
        {
            ToolTip.SetTip(column, Loc.Chrome("import.pane.lengths_differ"));
        }
        return column;
    }

    private static string Clock(ulong? milliseconds) =>
        milliseconds is { } value ? BridgeDisplay.Clock(value) : "—";

    // Auditioning the row, and whatever the row's disagreement leaves to do
    // about it: taking an unaccounted-for file out of the tracklist, or giving
    // an unanswered slot a file — or removing it.
    private Control Actions(int index, MappingSlotRow row)
    {
        var actions = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 5,
            VerticalAlignment = VerticalAlignment.Center,
        };

        if (row.LocalPath is not null)
        {
            var playing = _model.IsPlaying(index, _previewingPath());
            var play = ImportPaneUi.RowButton(
                Loc.Core(playing ? "ui.import.slots.stop" : "ui.import.slots.play"));
            play.Click += (_, _) =>
            {
                if (_model.IsPlaying(index, _previewingPath()))
                {
                    _onStop();
                }
                else
                {
                    _onPlay(index);
                }
            };
            actions.Children.Add(play);
        }

        switch (row.Kind)
        {
            case MappingSlotKind.Paired:
                actions.Children.Add(ChooseFileButton(index));
                break;
            case MappingSlotKind.FileOnly:
                actions.Children.Add(ChooseFileButton(index));
                var exclude = ImportPaneUi.RowButton(Loc.Core("ui.import.slots.exclude"));
                exclude.Click += (_, _) => _onExclude(index);
                actions.Children.Add(exclude);
                break;
            case MappingSlotKind.TrackOnly:
                actions.Children.Add(ChooseFileButton(index));
                var drop = ImportPaneUi.RowButton(Loc.Core("ui.import.slots.drop"));
                drop.Click += (_, _) => _onDrop(index);
                actions.Children.Add(drop);
                break;
            default:
                // A row of an import with no release mapped onto it. There is
                // no pairing to correct, so there is nothing to offer.
                break;
        }
        return actions;
    }

    // Re-pairing is this menu and nothing else — no drag gesture. It sits on
    // every row that has a slot, so one control covers giving an unanswered
    // slot a file, re-pointing a row that already has one, and swapping two
    // rows that came out the wrong way round: every case a drag would have
    // served. A drag needs a second hit target and a second interaction design
    // per toolkit, has no keyboard or accessibility path, and buys nothing this
    // does not already do.
    private Control ChooseFileButton(int index)
    {
        var button = ImportPaneUi.RowButton(Loc.Core("ui.import.slots.choose_file"));
        var items = _audio.Select(file =>
        {
            var item = new MenuItem { Header = $"{file.Name}  ·  {Loc.Bytes(checked((long)file.Size))}" };
            item.Click += (_, _) => _onChooseFile(index, file);
            return (Control)item;
        }).ToList();
        button.Flyout = new MenuFlyout { ItemsSource = items };
        button.IsEnabled = items.Count > 0;
        return button;
    }
}
