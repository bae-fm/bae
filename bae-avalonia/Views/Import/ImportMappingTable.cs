using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
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
internal sealed class ImportMappingTable
{
    private readonly BridgeMappingTable _table;
    private readonly Func<string, Task<List<ImportSheetBindingOption>>> _bindingOptions;
    private readonly Func<string?> _previewingPath;
    private readonly ImportMappingActions _actions;
    private readonly IReadOnlyList<ImportAudioChoice> _audioChoices;

    // The audio units nothing has read yet. Their rows say so while the read
    // runs; every other length in the table is already stored.
    private readonly IReadOnlyList<BridgeAudioFile> _unprobed;

    // The row hosts and the audio each one plays, in row order, so the accent on
    // the playing row moves without rebuilding the table under the fields the
    // user is typing in.
    private readonly List<(Border Host, string? AudioPath)> _rowHosts = new();

    // Every row's grid, the header's included, so the resolved widths reach all
    // of them at once when the pane is resized. One column grid means one list.
    private readonly List<Grid> _grids = new();

    internal ImportMappingTable(
        BridgeMappingTable table,
        Func<string, Task<List<ImportSheetBindingOption>>> bindingOptions,
        Func<string?> previewingPath,
        ImportMappingActions actions,
        IReadOnlyList<BridgeAudioFile>? unprobed = null)
    {
        _table = table;
        _bindingOptions = bindingOptions;
        _previewingPath = previewingPath;
        _actions = actions;
        _audioChoices = table.AudioChoices();
        _unprobed = unprobed ?? Array.Empty<BridgeAudioFile>();
    }

    internal Control Build()
    {
        _rowHosts.Clear();
        _grids.Clear();
        var column = new StackPanel { Spacing = 0, MinWidth = ImportMappingColumns.MinimumWidth };
        column.Children.Add(HeaderRow());
        foreach (var row in _table.Rows)
        {
            switch (row)
            {
                case BridgeMappingRow.Unit unit:
                    column.Children.Add(UnitRow(unit.UnitValue));
                    break;
                case BridgeMappingRow.Sheet sheet:
                    column.Children.Add(SheetRow(sheet.SheetValue));
                    foreach (var entry in sheet.Entries)
                    {
                        column.Children.Add(UnitRow(entry));
                    }
                    break;
                case BridgeMappingRow.Directory directory:
                    column.Children.Add(DirectoryRow(directory.DirectoryValue));
                    break;
                default:
                    throw new ArgumentOutOfRangeException(nameof(row), row, "Unknown mapping row");
            }
        }
        ApplyPreviewAccent();

        // A pane too narrow for the columns scrolls the table sideways. The
        // alternative is squeezing a column past the point it says anything —
        // or, as it stood, running the last two off the pane's right edge where
        // there is no way to reach them at all.
        var scroller = new ScrollViewer
        {
            Content = column,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
            VerticalScrollBarVisibility = ScrollBarVisibility.Disabled,
        };
        scroller.SizeChanged += (_, e) => ApplyColumns(e.NewSize.Width);
        ApplyColumns(scroller.Bounds.Width);
        return scroller;
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
            grid.ColumnDefinitions[0].Width = new GridLength(columns.Source);
            grid.ColumnDefinitions[1].Width = new GridLength(columns.Role);
            grid.ColumnDefinitions[3].Width = new GridLength(columns.Title);
            grid.ColumnDefinitions[4].Width = new GridLength(columns.Artist);
        }
    }

    /// <summary>The heading above the table, with the tally beside it. Absent for
    /// a folder there is nothing to reconcile against — no release is picked, or
    /// the tracklist was read off the folder's own files.</summary>
    internal Control Title() => ImportPaneUi.ZoneTitle(
        Loc.Core("ui.import.mapping.title"),
        _table.Reconciliation is { } reconciliation
            ? MappingTableReading.ReconciliationLine(reconciliation)
            : null);

    /// <summary>Move the accent onto whichever row the preview transport is
    /// playing. Called on every preview event; it touches only the row
    /// backgrounds, so a field mid-edit keeps its focus and its caret.</summary>
    internal void ApplyPreviewAccent()
    {
        var playing = _previewingPath();
        foreach (var (host, path) in _rowHosts)
        {
            if (playing is { Length: > 0 } && path == playing)
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
                new ColumnDefinition(new GridLength(start.Source)),
                new ColumnDefinition(new GridLength(start.Role)),
                new ColumnDefinition(new GridLength(ImportMappingColumns.Position)),
                new ColumnDefinition(new GridLength(start.Title)),
                new ColumnDefinition(new GridLength(start.Artist)),
                new ColumnDefinition(new GridLength(ImportMappingColumns.Length)),
                new ColumnDefinition(new GridLength(ImportMappingColumns.Actions)),
            },
            ColumnSpacing = ImportMappingColumns.Spacing,
        };
        _grids.Add(grid);
        return grid;
    }

    private Control HeaderRow()
    {
        var grid = Grid();
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.mapping.column.source"), 0));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.roles.column.role"), 1));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Chrome("edit.tracks.col_number"), 2));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.roles.column.becomes"), 3));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Chrome("edit.tracks.col_artist"), 4));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.slots.column.length"), 5));
        grid.Margin = new Thickness(0, 0, 0, 4);
        return grid;
    }

    // What every row sits in: one leading edge, one height, and a separator over
    // it. No striping — the columns are what a reader follows across a row, and a
    // tinted band under half of them is a second, competing grouping.
    private Border Host(Grid grid, string? audioPath)
    {
        var host = new Border
        {
            Padding = new Thickness(0, 6),
            Child = grid,
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0, _rowHosts.Count == 0 ? 0 : 1, 0, 0),
        };
        host[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        _rowHosts.Add((host, audioPath));
        return host;
    }

    // ── One source unit and what it becomes ──────────────────────────────────

    private Control UnitRow(BridgeMappingUnit unit)
    {
        var grid = Grid();

        var lengthsDiverge = LengthsDiverge(unit);

        var source = SourceCell(unit.Source, lengthsDiverge, IsMeasuring(unit.Source));
        Avalonia.Controls.Grid.SetColumn(source, 0);
        grid.Children.Add(source);

        if (RoleCell(unit.Source) is { } role)
        {
            Avalonia.Controls.Grid.SetColumn(role, 1);
            grid.Children.Add(role);
        }

        switch (unit.Becomes)
        {
            case BridgeMappingBecomes.Track track:
                AddTrackCells(grid, unit, track, lengthsDiverge);
                break;
            case BridgeMappingBecomes.Kept:
                AddBecomesText(grid, Loc.Core("ui.import.becomes.kept"));
                break;
            case BridgeMappingBecomes.AwaitingPick:
                AddBecomesText(grid, Loc.Core("ui.import.becomes.awaiting_pick"));
                break;
            default:
                throw new ArgumentOutOfRangeException(
                    nameof(unit), unit.Becomes, "Unknown mapping becomes");
        }

        return Host(grid, unit.Source.AudioPath());
    }

    /// <summary>Whether the folder and the release disagree about how long this
    /// row runs. Core decides how far apart is far enough — it is a judgement
    /// about how much two rips of one track may legitimately differ, and the
    /// other desktop surface has to reach the same answer.</summary>
    private static bool LengthsDiverge(BridgeMappingUnit unit) =>
        unit.Becomes is BridgeMappingBecomes.Track track
        && BaeBridgeMethods.BridgeLengthsDisagree(unit.Source.DurationMs(), track.SourceDurationMs);

    // The track this row commits, edited in place: the position the release
    // gives it, its title and artist, the release's length, and the one action
    // the row's own disagreement leaves to take.
    private void AddTrackCells(
        Grid grid,
        BridgeMappingUnit unit,
        BridgeMappingBecomes.Track becomes,
        bool lengthsDiverge)
    {
        var track = becomes.TrackValue;

        var position = ImportPaneUi.Cell(becomes.SourcePosition, secondary: true);
        Avalonia.Controls.Grid.SetColumn(position, 2);
        grid.Children.Add(position);

        var title = Field(track.Title, Loc.Core("ui.import.slots.untitled"));
        var artist = Field(track.ArtistText, Loc.Chrome("edit.tracks.artist_placeholder"));
        // Both fields write the row back whole. A keystroke does not rebuild the
        // table — the field being typed in has to keep its focus and its caret —
        // so each handler reads what its sibling currently holds rather than
        // what the row held when it was built, which is one edit out of date the
        // moment the other field is touched.
        void WriteBack() => _actions.EditTrack(track with
        {
            Title = title.Text ?? string.Empty,
            ArtistText = artist.Text ?? string.Empty,
        });
        title.TextChanged += (_, _) => WriteBack();
        artist.TextChanged += (_, _) => WriteBack();
        Avalonia.Controls.Grid.SetColumn(title, 3);
        grid.Children.Add(title);
        Avalonia.Controls.Grid.SetColumn(artist, 4);
        grid.Children.Add(artist);

        var length = new TextBlock
        {
            Text = MappingTableReading.DurationText(becomes.SourceDurationMs),
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
        Avalonia.Controls.Grid.SetColumn(length, 5);
        grid.Children.Add(length);

        var actions = RowActions(unit, track);
        Avalonia.Controls.Grid.SetColumn(actions, 6);
        grid.Children.Add(actions);
    }

    private static TextBox Field(string text, string watermark) => new()
    {
        Text = text,
        FontSize = 12.5,
        Watermark = watermark,
        VerticalAlignment = VerticalAlignment.Center,
    };

    // What a row that commits no track becomes, stated across the column the
    // editable rows put their title in so the two line up.
    private static void AddBecomesText(Grid grid, string text)
    {
        var cell = ImportPaneUi.Cell(text, secondary: true);
        Avalonia.Controls.Grid.SetColumn(cell, 3);
        grid.Children.Add(cell);
    }

    // Pick the audio this row writes, and the one action that belongs to the
    // row's own disagreement — Exclude for audio the release does not name, Drop
    // for a track this folder has nothing for.
    //
    // Re-pairing is the menu, not a drag. A drag needs a second hit target and a
    // second interaction design per toolkit, has no keyboard or accessibility
    // path, and buys nothing over picking from the folder's audio by name —
    // which is what re-pointing a row and swapping two rows both come down to.
    private Control RowActions(BridgeMappingUnit unit, BridgeRawTrackEdit track)
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

    // Whether this row's audio has not been read yet. Its length is the one
    // thing on the pane that is still being fetched, so it is the one place a
    // spinner belongs.
    private bool IsMeasuring(BridgeMappingSource source) =>
        source.Audio() is { } audio && _unprobed.Contains(audio);

    private Control SourceCell(
        BridgeMappingSource source, bool lengthsDiverge, bool isMeasuring) => source switch
        {
            BridgeMappingSource.File file => FileCell(file.FileValue, isMeasuring),
            BridgeMappingSource.SheetEntry entry => EntryCell(entry.Entry, lengthsDiverge, isMeasuring),
            BridgeMappingSource.Missing => ImportPaneUi.Cell(
                $"╌ {Loc.Core("ui.import.slots.no_file")}", secondary: true),
            _ => throw new ArgumentOutOfRangeException(nameof(source), source, "Unknown mapping source"),
        };

    // One of the folder's files, whole: its name in mono with its size after it,
    // the audition control where it is audio, and — where there is something to
    // open — opening it.
    private Control FileCell(BridgeMappingFile file, bool isMeasuring)
    {
        var line = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
        };
        var role = file.Role.FileRole();
        if (role is BridgeFileRole.Audio)
        {
            line.Children.Add(AuditionButton(file.LocalPath));
        }
        var name = ImportPaneUi.FileName(null, file.Name, checked((long)file.Size));
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
        if (isMeasuring)
        {
            line.Children.Add(MeasuringSpinner());
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
    private Control EntryCell(BridgeMappingEntry entry, bool lengthsDiverge, bool isMeasuring)
    {
        var line = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
        };
        line.Children.Add(AuditionButton(entry.ContainerLocalPath));

        var number = ImportPaneUi.Cell($"{entry.Number}.", secondary: true);
        number.FontFamily = new FontFamily("monospace");
        number.FontSize = 12;
        line.Children.Add(number);
        line.Children.Add(ImportPaneUi.Cell(entry.Title));

        if (isMeasuring)
        {
            line.Children.Add(MeasuringSpinner());
        }
        else
        {
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
        }
        return line;
    }

    // The one spinner on the pane: a row whose audio is still being read.
    private static Control MeasuringSpinner()
    {
        var spinner = new ProgressBar
        {
            IsIndeterminate = true,
            Width = 40,
            Height = 3,
            Margin = new Thickness(8, 0, 0, 0),
            VerticalAlignment = VerticalAlignment.Center,
        };
        ToolTip.SetTip(spinner, Loc.Chrome("import.measuring_length"));
        return spinner;
    }

    private Control AuditionButton(string path)
    {
        var playing = _previewingPath() is { Length: > 0 } current && current == path;
        var button = ImportPaneUi.RowButton(
            Loc.Core(playing ? "ui.import.slots.stop" : "ui.import.slots.play"));
        button.Click += (_, _) =>
        {
            if (_previewingPath() == path)
            {
                _actions.StopPreview();
            }
            else
            {
                _actions.Preview(path);
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
        return RoleControl(file.FileValue)
            ?? ImportPaneUi.Cell(Loc.Core(BaeBridgeMethods.BridgeFileRoleKey(file.FileValue.Role.FileRole())));
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
    private Control SheetRow(BridgeSheetGroup sheet)
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
        Avalonia.Controls.Grid.SetColumn(line, 0);
        grid.Children.Add(line);

        var disc = DiscControl(sheet);
        Avalonia.Controls.Grid.SetColumn(disc, 1);
        grid.Children.Add(disc);

        // The becomes half is the entries' to fill; the header only holds its
        // columns open so the group and its rows line up.
        return Host(grid, audioPath: null);
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
            null, directory.DirPrefix, checked((long)directory.TotalSize));
        Avalonia.Controls.Grid.SetColumn(name, 0);
        grid.Children.Add(name);
        var kind = ImportPaneUi.Cell(
            Loc.Core(
                BaeBridgeMethods.BridgeFileRowKindKey(directory.Kind),
                "count",
                (long)directory.Count),
            secondary: true);
        Avalonia.Controls.Grid.SetColumn(kind, 1);
        grid.Children.Add(kind);
        AddBecomesText(grid, Loc.Core("ui.import.becomes.kept"));
        return Host(grid, audioPath: null);
    }
}
