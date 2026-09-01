using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Section 2 of the mapping pane: every source unit the folder offers, paired
/// with the track committing makes of it.
///
/// One table, not two: the file a track comes from and the track it becomes are
/// the same row, so re-pointing, excluding, naming and role changes all happen
/// where the pairing is visible. A track sheet heads the group of entries it
/// carves; a collapsed directory is one row, because the roles of fourteen rip
/// logs are one fact.
/// </summary>
internal sealed partial class ImportMappingTable
{
    private readonly BridgeMappingTable _table;
    private readonly Func<string, Task<List<ImportSheetBindingOption>>> _bindingOptions;
    private readonly Func<BridgePreviewTarget?> _previewingTarget;
    private readonly ImportMappingActions _actions;
    private readonly LibraryService _library;
    private readonly IReadOnlyList<ImportAudioChoice> _audioChoices;

    // What identified the release, by the file each piece was read off. The row
    // for that file carries the chip.
    private readonly IReadOnlyList<BridgeFileEvidence> _evidence;

    // The row hosts and the audio each one plays, in row order, so the accent on
    // the playing row moves without rebuilding the table under the fields the
    // user is typing in.
    private readonly List<(Border Host, BridgePreviewTarget? Target)> _rowHosts = new();

    // Every row's grid, the header's included, so the resolved widths reach all
    // of them at once when the pane is resized. One column grid means one list.
    private readonly List<Grid> _grids = new();

    // Artist cells in their displayed order. The assignment is read through a
    // closure because editing the source cell does not rebuild the table.
    private readonly List<ArtistFillCell> _artistCells = new();
    private ArtistFillSelection? _artistFillSelection;
    private Canvas? _artistFillCanvas;
    private Border? _artistFillBorder;
    private Border? _artistFillHandle;
    private bool _draggingArtistFill;

    internal ImportMappingTable(
        BridgeMappingTable table,
        Func<string, Task<List<ImportSheetBindingOption>>> bindingOptions,
        Func<BridgePreviewTarget?> previewingTarget,
        LibraryService library,
        ImportMappingActions actions,
        IReadOnlyList<BridgeFileEvidence>? evidence = null)
    {
        _table = table;
        _bindingOptions = bindingOptions;
        _previewingTarget = previewingTarget;
        _library = library;
        _actions = actions;
        _audioChoices = table.AudioChoices();
        _evidence = evidence ?? Array.Empty<BridgeFileEvidence>();
    }

    internal Control Build()
    {
        _rowHosts.Clear();
        _grids.Clear();
        _artistCells.Clear();
        _artistFillSelection = null;
        var sections = new StackPanel { Spacing = 18 };
        sections.Children.Add(Section(
            Loc.Core("ui.import.mapping.tracks_title"),
            _table.Reconciliation is { } reconciliation
                ? MappingTableReading.ReconciliationLine(reconciliation)
                : null,
            TrackHeaderRow(),
            TrackRows(),
            supportsArtistFill: true));
        var files = FileRows();
        if (files.Count > 0)
        {
            sections.Children.Add(Section(
                Loc.Core("ui.import.mapping.files_title"),
                null,
                FileHeaderRow(),
                files));
        }
        ApplyPreviewAccent();
        return sections;
    }

    /// <summary>The rows that become tracks. A sheet heads the run of slices it
    /// carves; the slices are tracks like any other and carry no sheet controls
    /// of their own.</summary>
    private List<Control> TrackRows()
    {
        var rows = new List<Control>();
        foreach (var group in _table.TrackGroups)
        {
            switch (group)
            {
                case BridgeMappingTrackGroup.Unit unit:
                    rows.Add(TrackRow(unit.UnitValue));
                    break;
                case BridgeMappingTrackGroup.Sheet sheet:
                    rows.Add(SheetRow(sheet.SheetValue, headsTracks: true));
                    foreach (var entry in sheet.Entries)
                    {
                        rows.Add(TrackRow(entry));
                    }
                    break;
            }
        }
        return rows;
    }

    /// <summary>The rows carried with the release that are not its tracks.
    /// Being listed here with a role is the whole statement — there is no
    /// sentence saying they are kept, because the section they are in says
    /// it.</summary>
    private List<Control> FileRows()
    {
        var rows = new List<Control>();
        foreach (var row in _table.Files)
        {
            switch (row)
            {
                case BridgeMappingFileRow.File file:
                    rows.Add(FileRow(file.FileValue));
                    break;
                case BridgeMappingFileRow.Sheet sheet:
                    rows.Add(SheetRow(sheet.SheetValue, headsTracks: false));
                    break;
                case BridgeMappingFileRow.Directory directory:
                    rows.Add(DirectoryRow(directory.DirectoryValue));
                    break;
            }
        }
        return rows;
    }

    /// <summary>One titled card of rows. A pane too narrow for the columns
    /// scrolls sideways rather than squeezing a column past the point it says
    /// anything, and both sections resolve against the same width so their
    /// columns stay aligned.</summary>
    private Control Section(
        string title,
        string? trailing,
        Control header,
        List<Control> rows,
        bool supportsArtistFill = false)
    {
        var column = new StackPanel { Spacing = 0, MinWidth = ImportMappingColumns.MinimumWidth };
        column.Children.Add(header);
        foreach (var row in rows)
        {
            column.Children.Add(row);
        }
        Control content = column;
        if (supportsArtistFill)
        {
            var layers = new Grid { MinWidth = ImportMappingColumns.MinimumWidth };
            layers.Children.Add(column);
            layers.Children.Add(ArtistFillOverlay());
            column.LayoutUpdated += (_, _) => UpdateArtistFillOverlay();
            content = layers;
        }
        var scroller = new ScrollViewer
        {
            Content = content,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
            VerticalScrollBarVisibility = ScrollBarVisibility.Disabled,
        };
        scroller.SizeChanged += (_, e) => ApplyColumns(e.NewSize.Width);
        ApplyColumns(scroller.Bounds.Width);
        var section = new StackPanel { Spacing = 8 };
        section.Children.Add(ImportPaneUi.ZoneTitle(title, trailing));
        section.Children.Add(scroller);
        return section;
    }

    /// <summary>Resolve the columns against the width the pane leaves the table
    /// and write them onto every row's grid, the header's included. The table is
    /// laid out at this width or at its own minimum, whichever is wider — so the
    /// pane never has more table than it has room for, and the row never has
    /// less than its columns need.</summary>
    private void ApplyColumns(double paneWidth)
    {
        var columns = ImportMappingColumns.Resolve(paneWidth);
        foreach (var grid in _grids)
        {
            grid.ColumnDefinitions[1].Width = new GridLength(columns.Title);
            grid.ColumnDefinitions[2].Width = new GridLength(columns.Artist);
            grid.ColumnDefinitions[4].Width = new GridLength(columns.Source);
        }
    }

    /// <summary>Accent the row whose audio is auditioning, and clear every
    /// other. Applied in place rather than by rebuilding, so a preview starting
    /// does not take the focus out of a field being typed in.</summary>
    internal void ApplyPreviewAccent()
    {
        var playing = _previewingTarget();
        foreach (var (host, target) in _rowHosts)
        {
            if (playing is not null && target == playing)
            {
                host[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeSelectionTintBrush");
            }
            else
            {
                host.Background = Brushes.Transparent;
            }
        }
    }

    // ── The grid every row shares ────────────────────────────────────────────

    // Source, the role in force, the position the release gives the row, the two
    // editable fields, the release's length, and the row's own actions.
    //
    // Every width is the table's, so a header sits over its own column's content
    // on every row: a row negotiating its own widths against its own content is
    // what puts one row's length under another row's role. The four that give
    // are written in by ApplyColumns once the pane's width is known; they start
    // at the narrowest the table is ever laid out at.
    private Grid Grid()
    {
        var start = ImportMappingColumns.Resolve(ImportMappingColumns.MinimumWidth);
        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions
            {
                new ColumnDefinition(new GridLength(ImportMappingColumns.Position)),
                new ColumnDefinition(new GridLength(start.Title)),
                new ColumnDefinition(new GridLength(start.Artist)),
                new ColumnDefinition(new GridLength(ImportMappingColumns.Length)),
                new ColumnDefinition(new GridLength(start.Source)),
            },
            ColumnSpacing = ImportMappingColumns.Spacing,
        };
        _grids.Add(grid);
        return grid;
    }

    private Control TrackHeaderRow()
    {
        var grid = Grid();
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Chrome("edit.tracks.col_number"), 0));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.mapping.column.title"), 1));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.mapping.column.artist"), 2));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.slots.column.length"), 3));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.mapping.column.source"), 4));
        grid.Margin = new Thickness(0, 0, 0, 4);
        return grid;
    }

    private Control FileHeaderRow()
    {
        var grid = Grid();
        var name = ImportPaneUi.ColumnHeader(Loc.Core("ui.import.mapping.column.name"), 0);
        Avalonia.Controls.Grid.SetColumnSpan(name, 3);
        grid.Children.Add(name);
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Chrome("storage.column.size"), 3));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.roles.column.role"), 4));
        grid.Margin = new Thickness(0, 0, 0, 4);
        return grid;
    }

    // What every row sits in: one leading edge, one height, and a separator over
    // it. No striping — the columns are what a reader follows across a row, and a
    // tinted band under half of them is a second, competing grouping.
    private Border HostOf(Grid grid, BridgePreviewTarget? target)
    {
        var host = new Border
        {
            Padding = new Thickness(0, 6),
            Child = grid,
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0, _rowHosts.Count == 0 ? 0 : 1, 0, 0),
        };
        host[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        _rowHosts.Add((host, target));
        return host;
    }

    // ── One of the release's tracks ──────────────────────────────────────────

    // The track's number, what it will be called, who it is by, how long it
    // runs, and the file behind it. The control that re-points the row at
    // another file is not a column: it appears over the Source cell when the
    // pointer is on the row, and stays put on a row that has no file, which is
    // the one row that has to be answered.
    private Control TrackRow(BridgeMappingUnit unit)
    {
        var grid = Grid();
        var lengthsDiverge = LengthsDiverge(unit);
        var track = (unit.Becomes as BridgeMappingBecomes.Track)?.TrackValue;

        if (unit.Becomes is BridgeMappingBecomes.Track becomes)
        {
            AddTrackCells(grid, becomes);
        }
        else
        {
            var waiting = ImportPaneUi.Cell(
                Loc.Core("ui.import.becomes.awaiting_pick"), secondary: true);
            Avalonia.Controls.Grid.SetColumn(waiting, 1);
            grid.Children.Add(waiting);
        }
        AddDurationCell(grid, unit, lengthsDiverge);

        var source = SourceCell(unit.Source, lengthsDiverge);
        var cell = new Panel();
        cell.Children.Add(source);
        if (track is { } editable)
        {
            var actions = RowActions(unit, editable);
            actions.HorizontalAlignment = HorizontalAlignment.Right;
            actions.VerticalAlignment = VerticalAlignment.Center;
            // A row with no file behind it is the one that has to be answered,
            // so its picker does not wait to be hovered.
            actions.IsVisible = NeedsAnswer(unit, editable);
            cell.Children.Add(actions);
            var host = HostOf(grid, unit.Source.PreviewTarget());
            host.PointerEntered += (_, _) => actions.IsVisible = true;
            host.PointerExited += (_, _) =>
                actions.IsVisible = NeedsAnswer(unit, editable);
            Avalonia.Controls.Grid.SetColumn(cell, 4);
            grid.Children.Add(cell);
            return host;
        }
        Avalonia.Controls.Grid.SetColumn(cell, 4);
        grid.Children.Add(cell);
        return HostOf(grid, unit.Source.PreviewTarget());
    }

    private static bool NeedsAnswer(BridgeMappingUnit unit, BridgeRawTrackEdit track) =>
        unit.Source is BridgeMappingSource.Missing || track.File is null;

    // ── One file carried with the release ────────────────────────────────────

    // A rip log, a text file, audio somebody took out of the tracklist: its
    // name, how big it is, and the job it has. Nothing says it is kept — it is
    // listed under a heading that says Files, which is the same statement
    // without the sentence.
    private Control FileRow(BridgeMappingFile file)
    {
        var grid = Grid();
        var source = FileCell(file, showsSize: false);
        Avalonia.Controls.Grid.SetColumn(source, 0);
        Avalonia.Controls.Grid.SetColumnSpan(source, 3);
        grid.Children.Add(source);
        var size = ImportPaneUi.Cell(Loc.Bytes(checked((long)file.Size)), secondary: true);
        size.HorizontalAlignment = HorizontalAlignment.Right;
        Avalonia.Controls.Grid.SetColumn(size, 3);
        grid.Children.Add(size);
        var role = RoleControl(file) ?? RoleChip(file.Role.FileRole());
        Avalonia.Controls.Grid.SetColumn(role, 4);
        grid.Children.Add(role);
        return HostOf(grid, file.PreviewTarget);
    }

    /// <summary>Whether the folder and the release disagree about how long this
    /// row runs. Core decides how far apart is far enough — it is a judgement
    /// about how much two rips of one track may legitimately differ, and the
    /// other desktop surface has to reach the same answer.</summary>
    private static bool LengthsDiverge(BridgeMappingUnit unit) =>
        unit.Becomes is BridgeMappingBecomes.Track
        && BaeBridgeMethods.BridgeLengthsDisagree(unit.Source.DurationMs(), unit.DurationMs);

    // The track this row commits, edited in place: the position the release
    // gives it, its title and artist. Length belongs to every audio row,
    // including one that is still waiting for metadata, so the row owner adds
    // it after these metadata cells.
    private void AddTrackCells(Grid grid, BridgeMappingBecomes.Track becomes)
    {
        var track = becomes.TrackValue;

        var position = ImportPaneUi.Cell(becomes.SourcePosition, secondary: true);
        Avalonia.Controls.Grid.SetColumn(position, 0);
        grid.Children.Add(position);

        var title = Field(track.Title, Loc.Core("ui.import.slots.untitled"));
        var currentArtistAssignments = track.ArtistAssignments;
        var explicitArtists = currentArtistAssignments as BridgeTrackArtistAssignments.Explicit;
        var artist = new ArtistAssignmentsField(
            explicitArtists?.Assignments ?? Array.Empty<BridgeArtistAssignment>(),
            _library,
            assignments =>
            {
                currentArtistAssignments = new BridgeTrackArtistAssignments.Explicit(
                    assignments.ToArray());
                WriteBack();
            },
            inheritsAlbumArtists: currentArtistAssignments is BridgeTrackArtistAssignments.AlbumArtists,
            onUseAlbumArtists: () =>
            {
                currentArtistAssignments = new BridgeTrackArtistAssignments.AlbumArtists();
                WriteBack();
            });
        // Both fields write the row back whole. A keystroke does not rebuild the
        // table — the field being typed in has to keep its focus and its caret —
        // so each handler reads what its sibling currently holds rather than
        // what the row held when it was built, which is one edit out of date the
        // moment the other field is touched.
        void WriteBack() => _actions.EditTrack(track with
        {
            Title = title.Text ?? string.Empty,
            ArtistAssignments = currentArtistAssignments,
        });
        title.TextChanged += (_, _) => WriteBack();
        Avalonia.Controls.Grid.SetColumn(title, 1);
        grid.Children.Add(title);
        var artistCell = new Border
        {
            Background = Brushes.Transparent,
            Child = artist,
        };
        var artistIndex = _artistCells.Count;
        artistCell.AddHandler(
            InputElement.PointerPressedEvent,
            (_, e) =>
            {
                if (e.GetCurrentPoint(artistCell).Properties.IsLeftButtonPressed)
                {
                    _artistFillSelection = new ArtistFillSelection(artistIndex);
                    UpdateArtistFillOverlay();
                }
            },
            RoutingStrategies.Tunnel);
        _artistCells.Add(new ArtistFillCell(
            track.Id,
            artistCell,
            () => currentArtistAssignments));
        Avalonia.Controls.Grid.SetColumn(artistCell, 2);
        grid.Children.Add(artistCell);
    }

    private static void AddDurationCell(
        Grid grid,
        BridgeMappingUnit unit,
        bool lengthsDiverge)
    {
        var length = new TextBlock
        {
            Text = MappingTableReading.DurationText(unit.DurationMs),
            FontSize = 12,
            FontFamily = new FontFamily("monospace"),
            HorizontalAlignment = HorizontalAlignment.Right,
            VerticalAlignment = VerticalAlignment.Center,
        };
        length[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(
            lengthsDiverge ? "BaeWarningBrush" : "BaeTextPrimaryBrush");
        if (lengthsDiverge)
        {
            ToolTip.SetTip(length, Loc.Chrome("import.pane.lengths_differ"));
        }
        Avalonia.Controls.Grid.SetColumn(length, 3);
        grid.Children.Add(length);
    }

    private static TextBox Field(string text, string watermark) => new()
    {
        Text = text,
        FontSize = 12.5,
        Watermark = watermark,
        VerticalAlignment = VerticalAlignment.Center,
    };

    // Pick the audio this row writes, and the one action that belongs to the
    // row's own disagreement — Exclude for audio the release does not name, Drop
    // for a track this folder has nothing for.
    //
    // Re-pairing is the menu, not a drag. A drag needs a second hit target and a
    // second interaction design per toolkit, has no keyboard or accessibility
    // path, and buys nothing over picking from the folder's audio by name —
    // which is what re-pointing a row and swapping two rows both come down to.
    private StackPanel RowActions(BridgeMappingUnit unit, BridgeRawTrackEdit track)
    {
        var actions = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 5,
            VerticalAlignment = VerticalAlignment.Center,
        };
        if (_audioChoices.Count > 0)
        {
            actions.Children.Add(ChooseFileButton(track));
        }
        if (unit.Becomes is BridgeMappingBecomes.Track { SourcePosition: null }
            && unit.Source is BridgeMappingSource.File file)
        {
            var exclude = ImportPaneUi.RowButton(Loc.Core("ui.import.slots.exclude"));
            var fileId = file.FileValue.FileId;
            exclude.Click += (_, _) => _actions.Exclude(fileId);
            actions.Children.Add(exclude);
        }
        else if (track.File is null)
        {
            var drop = ImportPaneUi.RowButton(Loc.Core("ui.import.slots.drop"));
            drop.Click += (_, _) => _actions.Drop(track.Id);
            actions.Children.Add(drop);
        }
        return actions;
    }

    private Control ChooseFileButton(BridgeRawTrackEdit track)
    {
        var button = ImportPaneUi.RowButton(Loc.Core("ui.import.slots.choose_file"));
        var items = _audioChoices.Select(choice =>
        {
            var item = new MenuItem { Header = choice.Label };
            item.Click += (_, _) => _actions.ChooseFile(track.Id, choice.Audio);
            return (Control)item;
        }).ToList();
        button.Flyout = new MenuFlyout { ItemsSource = items };
        return button;
    }

    // ── The left half ────────────────────────────────────────────────────────

    private Control SourceCell(
        BridgeMappingSource source, bool lengthsDiverge) => source switch
        {
            BridgeMappingSource.File file => FileCell(file.FileValue, showsSize: true),
            BridgeMappingSource.SheetEntry entry => EntryCell(entry.Entry, lengthsDiverge),
            BridgeMappingSource.Missing => ImportPaneUi.Cell(
                $"╌ {Loc.Core("ui.import.slots.no_file")}", secondary: true),
            _ => throw new ArgumentOutOfRangeException(nameof(source), source, "Unknown mapping source"),
        };

    // One of the folder's files, whole: its name in mono with its size after it,
    // the audition control where it is audio, and — where there is something to
    // open — opening it.
    private Control FileCell(BridgeMappingFile file, bool showsSize)
    {
        var line = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
        };
        var role = file.Role.FileRole();
        if (file.PreviewTarget is { } target)
        {
            line.Children.Add(AuditionButton(target));
        }
        var name = ImportPaneUi.FileName(
            null,
            file.Name,
            checked((long)file.Size),
            showsSize);
        if (OpenAction(file, role) is { } open)
        {
            var button = new Button
            {
                Content = name,
                Background = Brushes.Transparent,
                BorderThickness = new Thickness(0),
                Padding = new Thickness(0),
                Cursor = new Avalonia.Input.Cursor(Avalonia.Input.StandardCursorType.Hand),
            };
            button.Click += (_, _) => open();
            line.Children.Add(button);
        }
        else
        {
            line.Children.Add(name);
        }
        if (file.AudioFormat is { } audioFormat)
        {
            line.Children.Add(ImportPaneUi.Cell(
                BridgeDisplay.AudioFormat(audioFormat), secondary: true));
        }
        if (ImportEvidence.Of(file.FileId, _evidence) is { } found)
        {
            line.Children.Add(ImportEvidence.Chip(found));
        }
        return line;
    }

    // A readable file opens in the document viewer; audio has nothing to open,
    // only to play, and the images are the gallery's.
    private Action? OpenAction(BridgeMappingFile file, BridgeFileRole role) => role switch
    {
        BridgeFileRole.Document => () => _actions.OpenDocument(file.Name, file.LocalPath),
        _ => null,
    };

    // One entry of a track sheet: the number it prints, the title it gives, and
    // how long it says the entry runs. The audio is the container's, which is the
    // only file on disk there is to audition.
    private Control EntryCell(BridgeMappingEntry entry, bool lengthsDiverge)
    {
        var line = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
        };
        line.Children.Add(AuditionButton(entry.PreviewTarget));

        var number = ImportPaneUi.Cell($"{entry.Number}.", secondary: true);
        number.FontFamily = new FontFamily("monospace");
        number.FontSize = 12;
        line.Children.Add(number);
        line.Children.Add(ImportPaneUi.Cell(entry.Title));
        line.Children.Add(ImportPaneUi.Cell(
            BridgeDisplay.AudioFormat(entry.AudioFormat), secondary: true));

        var length = new TextBlock
        {
            Text = MappingTableReading.DurationText(entry.DurationMs),
            FontSize = 11.5,
            FontFamily = new FontFamily("monospace"),
            Margin = new Thickness(8, 0, 0, 0),
            VerticalAlignment = VerticalAlignment.Center,
        };
        length[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(
            lengthsDiverge ? "BaeWarningBrush" : "BaeTextSecondaryBrush");
        line.Children.Add(length);
        return line;
    }

    private Control AuditionButton(BridgePreviewTarget target)
    {
        var playing = _previewingTarget() == target;
        var button = ImportPaneUi.RowButton(
            Loc.Core(playing ? "ui.import.slots.stop" : "ui.import.slots.play"));
        button.Click += (_, _) =>
        {
            if (_previewingTarget() == target)
            {
                _actions.StopPreview();
            }
            else
            {
                _actions.Preview(target);
            }
        };
        return button;
    }

    // The job in force for a file, and the control that changes it where the job
    // is a decision. A sheet's entries carry no role of their own — their group
    // header holds the sheet's decisions — and neither does a track the folder
    // has nothing for.
    private Control? RoleCell(BridgeMappingSource source)
    {
        if (source is not BridgeMappingSource.File file)
        {
            return null;
        }
        return RoleControl(file.FileValue) ?? RoleChip(file.FileValue.Role.FileRole());
    }

    /// <summary>The job one file has, as a chip: what the Role column holds
    /// where the role is nobody's choice to make.</summary>
    private static Control RoleChip(BridgeFileRole role)
    {
        var text = new TextBlock
        {
            Text = Loc.Core(BaeBridgeMethods.BridgeFileRoleKey(role)),
            FontSize = 11,
            FontWeight = FontWeight.Medium,
            VerticalAlignment = VerticalAlignment.Center,
        };
        text[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var chip = new Border
        {
            CornerRadius = new CornerRadius(10),
            Padding = new Thickness(6, 1),
            HorizontalAlignment = HorizontalAlignment.Left,
            VerticalAlignment = VerticalAlignment.Center,
            Child = text,
        };
        chip[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        return chip;
    }

    // Present only where core offered alternatives, which is every file the scan
    // read as audio and nothing else: an image is an image, and a track sheet's
    // job is decided by what it is bound to. A file already out of the tracklist
    // gets the shorthand action instead of a two-item menu.
    private Control? RoleControl(BridgeMappingFile file)
    {
        if (file.Alternatives.Length == 0)
        {
            return null;
        }
        if (file.RoleChoice == BridgeFileRoleChoice.NotATrack)
        {
            var putBack = ImportPaneUi.RowButton(Loc.Core("ui.import.slots.put_back"));
            putBack.Click += (_, _) => _actions.SetRole(file.FileId, BridgeFileRoleChoice.Audio);
            return putBack;
        }

        var button = ImportPaneUi.RowButton(Loc.Core(BaeBridgeMethods.BridgeFileRoleChoiceKey(
            file.RoleChoice ?? BridgeFileRoleChoice.Audio)));
        var items = file.Alternatives.Select(choice =>
        {
            var item = new MenuItem
            {
                Header = (choice == file.RoleChoice ? "✓ " : string.Empty)
                    + Loc.Core(BaeBridgeMethods.BridgeFileRoleChoiceKey(choice)),
            };
            item.Click += (_, _) => _actions.SetRole(file.FileId, choice);
            return (Control)item;
        }).ToList();
        button.Flyout = new MenuFlyout { ItemsSource = items };
        return button;
    }

    // ── A track sheet, heading the group of entries it carves ────────────────

    // The two controls are the sheet's decisions and nothing else's. Which audio
    // a sheet speaks for is one — a FILE directive naming a file that was later
    // re-encoded under another name has no answer but the user's. Which disc it
    // is is the other: cue filenames are arbitrary, CD1.cue may hold disc two, so
    // the assignment is the truth and no name is read for it.
    private Control SheetRow(BridgeSheetGroup sheet, bool headsTracks)
    {
        var grid = Grid();

        var line = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
        };
        var name = new TextBlock
        {
            Text = sheet.Name,
            FontFamily = new FontFamily("monospace"),
            FontSize = 12,
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
        };
        name[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var open = new Button
        {
            Content = name,
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(0),
            Cursor = new Avalonia.Input.Cursor(Avalonia.Input.StandardCursorType.Hand),
        };
        open.Click += (_, _) => _actions.OpenDocument(sheet.Name, sheet.LocalPath);
        line.Children.Add(open);
        line.Children.Add(SheetBindingControl(sheet));
        // Why a sheet is on nothing, where it is on nothing: the directive's own
        // text, or the codec bae cannot carve.
        if (BridgeDisplay.UnboundSheetLine(sheet.Bound) is { } reason)
        {
            line.Children.Add(ImportPaneUi.Cell(reason, secondary: true));
        }
        if (sheet.Bound.Container() is { } container)
        {
            line.Children.Add(ImportPaneUi.Cell(
                Loc.Bytes(checked((long)container.Size)), secondary: true));
        }
        if (ImportEvidence.Of(sheet.SheetId, _evidence) is { } found)
        {
            line.Children.Add(ImportEvidence.Chip(found));
        }
        Avalonia.Controls.Grid.SetColumn(line, 0);
        Avalonia.Controls.Grid.SetColumnSpan(line, 3);
        grid.Children.Add(line);

        if (!headsTracks)
        {
            var size = ImportPaneUi.Cell(
                Loc.Bytes(checked((long)sheet.Size)), secondary: true);
            size.HorizontalAlignment = HorizontalAlignment.Right;
            Avalonia.Controls.Grid.SetColumn(size, 3);
            grid.Children.Add(size);
        }

        // The slices below fill the length; the header only holds the column
        // open so the group and its rows line up.
        var disc = DiscControl(sheet);
        Avalonia.Controls.Grid.SetColumn(disc, 4);
        grid.Children.Add(disc);
        var host = HostOf(grid, target: null);
        if (headsTracks)
        {
            host[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
            host.Padding = new Thickness(0, 8);
        }
        return host;
    }

    /// <summary>Which of the release's discs a sheet's entries are, or that it
    /// contributes nothing.</summary>
    private Control DiscControl(BridgeSheetGroup sheet)
    {
        var button = ImportPaneUi.RowButton(DiscLabel(sheet.Assignment));
        ToolTip.SetTip(button, Loc.Core("ui.import.sheet.disc_help"));
        var items = sheet.DiscOptions
            .Select(number => Item(
                Loc.Core("ui.import.sheet.disc", "number", (long)number),
                sheet.Assignment is BridgeSheetDisc.Disc disc && disc.Number == number,
                new BridgeSheetDisc.Disc(number)))
            .Append(Item(
                Loc.Core("ui.import.sheet.ignored"),
                sheet.Assignment is BridgeSheetDisc.Ignored,
                new BridgeSheetDisc.Ignored()))
            .ToList();
        button.Flyout = new MenuFlyout { ItemsSource = items };
        return button;

        Control Item(string label, bool selected, BridgeSheetDisc assignment)
        {
            var item = new MenuItem { Header = label, Icon = selected ? new TextBlock { Text = "✓" } : null };
            item.Click += (_, _) => _actions.SetSheetDisc(sheet.SheetId, assignment);
            return item;
        }
    }

    private static string DiscLabel(BridgeSheetDisc assignment) => assignment switch
    {
        BridgeSheetDisc.Disc disc => Loc.Core("ui.import.sheet.disc", "number", (long)disc.Number),
        BridgeSheetDisc.Ignored => Loc.Core("ui.import.sheet.ignored"),
        _ => throw new ArgumentOutOfRangeException(
            nameof(assignment), assignment, "Unknown sheet disc"),
    };

    // What a track sheet describes, and the picker that names it. The choices
    // come from core already filtered to what the sheet can use, each refusal
    // carrying its reason — offering a file the commit would reject is the
    // failure the editable binding exists to remove.
    private Control SheetBindingControl(BridgeSheetGroup sheet)
    {
        var combo = new ComboBox
        {
            FontSize = 12,
            MinWidth = 150,
            PlaceholderText = Loc.Core("ui.import.sheet.choose_audio"),
            VerticalAlignment = VerticalAlignment.Center,
        };
        // Repopulating sets SelectedItem, which raises SelectionChanged; without
        // this the initial fill would read as the user picking what is already
        // bound and write it back.
        var filling = false;
        var bound = sheet.Bound.Container()?.FileId;
        combo.SelectionChanged += (_, _) =>
        {
            if (filling || combo.SelectedItem is not ComboBoxItem { Tag: string[] tag })
            {
                return;
            }
            var audioFileId = tag.Length == 0 ? null : tag[0];
            if (audioFileId == bound)
            {
                return;
            }
            _actions.BindSheet(sheet.SheetId, audioFileId);
        };

        // Core probes every audio file to answer, so the choices are read once,
        // when the row is built.
        _ = FillOptions(combo, sheet.SheetId, bound, () => filling = true, () => filling = false);
        return combo;
    }

    private async Task FillOptions(
        ComboBox combo, string sheetFileId, string? bound, Action startFilling, Action doneFilling)
    {
        var options = await _bindingOptions(sheetFileId);
        startFilling();
        combo.Items.Clear();
        ComboBoxItem? selected = null;
        foreach (var option in options)
        {
            var item = new ComboBoxItem
            {
                Content = option.RefusalReason is null
                    ? option.FileId
                    : $"{option.FileId}  ·  {option.RefusalReason}",
                // A file the sheet cannot use is shown, disabled, with core's
                // reason — a folder whose only audio is unusable reads as "here
                // is why" rather than as an empty list.
                IsEnabled = option.RefusalReason is null,
                Tag = new[] { option.FileId },
            };
            combo.Items.Add(item);
            if (option.FileId == bound)
            {
                selected = item;
            }
        }
        if (options.Count > 0)
        {
            var nothing = new ComboBoxItem
            {
                Content = Loc.Core("ui.import.sheet.describes_nothing"),
                Tag = Array.Empty<string>(),
            };
            combo.Items.Add(nothing);
            selected ??= bound is null ? nothing : null;
        }
        // Nothing to offer: a sheet naming one file per track, or a folder with
        // no audio. There is no choice to present, so there is no control.
        combo.IsVisible = options.Count > 0;
        combo.SelectedItem = selected;
        doneFilling();
    }

    // ── A directory whose files all do the same job ──────────────────────────

    // As the one row core decided it should be, each fact under the header it
    // belongs to: the directory and its size where a file's name and size go,
    // what it holds where a role goes, and what becomes of it where every other
    // row says so.
    private Control DirectoryRow(BridgeCollapsedDirectory directory)
    {
        var grid = Grid();
        var name = ImportPaneUi.FileName(
            null,
            directory.DirPrefix,
            checked((long)directory.TotalSize),
            showsSize: false);
        Avalonia.Controls.Grid.SetColumn(name, 0);
        Avalonia.Controls.Grid.SetColumnSpan(name, 3);
        grid.Children.Add(name);
        var size = ImportPaneUi.Cell(
            Loc.Bytes(checked((long)directory.TotalSize)), secondary: true);
        size.HorizontalAlignment = HorizontalAlignment.Right;
        Avalonia.Controls.Grid.SetColumn(size, 3);
        grid.Children.Add(size);
        var kind = ImportPaneUi.Cell(
            Loc.Core(
                BaeBridgeMethods.BridgeFileRowKindKey(directory.Kind),
                "count",
                (long)directory.Count),
            secondary: true);
        Avalonia.Controls.Grid.SetColumn(kind, 4);
        grid.Children.Add(kind);
        return HostOf(grid, target: null);
    }
}
