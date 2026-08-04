using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
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
/// carves; the folder's images are one gallery row; a collapsed directory is one
/// row, because the roles of fourteen rip logs are one fact.
/// </summary>
internal sealed class ImportMappingTable
{
    /// <summary>How far a track sheet's entries sit inside their group
    /// header.</summary>
    private const double EntryIndent = 18;

    private readonly BridgeMappingTable _table;
    private readonly Func<string, Task<List<ImportSheetBindingOption>>> _bindingOptions;
    private readonly Func<string?> _previewingPath;
    private readonly Action<Image, string> _loadImage;
    private readonly ImportMappingActions _actions;
    private readonly IReadOnlyList<ImportAudioChoice> _audioChoices;

    // The row hosts and the audio each one plays, in row order, so the accent on
    // the playing row moves without rebuilding the table under the fields the
    // user is typing in.
    private readonly List<(Border Host, string? AudioPath)> _rowHosts = new();

    internal ImportMappingTable(
        BridgeMappingTable table,
        Func<string, Task<List<ImportSheetBindingOption>>> bindingOptions,
        Func<string?> previewingPath,
        Action<Image, string> loadImage,
        ImportMappingActions actions)
    {
        _table = table;
        _bindingOptions = bindingOptions;
        _previewingPath = previewingPath;
        _loadImage = loadImage;
        _actions = actions;
        _audioChoices = table.AudioChoices();
    }

    internal Control Build()
    {
        _rowHosts.Clear();
        var column = new StackPanel { Spacing = 0 };
        column.Children.Add(HeaderRow());
        foreach (var row in _table.Rows)
        {
            switch (row)
            {
                case BridgeMappingRow.Unit unit:
                    column.Children.Add(UnitRow(unit.UnitValue, indent: 0));
                    break;
                case BridgeMappingRow.Sheet sheet:
                    column.Children.Add(SheetRow(sheet.SheetValue));
                    foreach (var entry in sheet.Entries)
                    {
                        column.Children.Add(UnitRow(entry, EntryIndent));
                    }
                    break;
                case BridgeMappingRow.Images images:
                    column.Children.Add(ImagesRow(images.ImagesValue));
                    break;
                case BridgeMappingRow.Directory directory:
                    column.Children.Add(DirectoryRow(directory.DirectoryValue));
                    break;
                default:
                    throw new ArgumentOutOfRangeException(nameof(row), row, "Unknown mapping row");
            }
        }
        ApplyPreviewAccent();
        return column;
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
    private static Grid Grid() => new()
    {
        ColumnDefinitions = new ColumnDefinitions("*,118,34,*,*,64,Auto"),
        ColumnSpacing = 8,
    };

    private static Control HeaderRow()
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

    private Border Host(Grid grid, string? audioPath)
    {
        var host = new Border
        {
            Padding = new Thickness(4),
            CornerRadius = new CornerRadius(6),
            Child = grid,
            Background = Brushes.Transparent,
        };
        _rowHosts.Add((host, audioPath));
        return host;
    }

    // ── One source unit and what it becomes ──────────────────────────────────

    private Control UnitRow(BridgeMappingUnit unit, double indent)
    {
        var grid = Grid();

        var lengthsDiverge = LengthsDiverge(unit);

        var source = SourceCell(unit.Source, lengthsDiverge);
        source.Margin = new Thickness(indent, 0, 0, 0);
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

    private Control SourceCell(BridgeMappingSource source, bool lengthsDiverge) => source switch
    {
        BridgeMappingSource.File file => FileCell(file.FileValue),
        BridgeMappingSource.SheetEntry entry => EntryCell(entry.Entry, lengthsDiverge),
        BridgeMappingSource.Missing => ImportPaneUi.Cell(
            $"╌ {Loc.Core("ui.import.slots.no_file")}", secondary: true),
        _ => throw new ArgumentOutOfRangeException(nameof(source), source, "Unknown mapping source"),
    };

    // One of the folder's files, whole: its name in mono with its size after it,
    // the audition control where it is audio, and — where there is something to
    // open — opening it.
    private Control FileCell(BridgeMappingFile file)
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
        line.Children.Add(AuditionButton(entry.ContainerLocalPath));

        var number = ImportPaneUi.Cell($"{entry.Number}.", secondary: true);
        number.FontFamily = new FontFamily("monospace");
        number.FontSize = 12;
        line.Children.Add(number);
        line.Children.Add(ImportPaneUi.Cell(entry.Title));

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

    // ── The folder's images, as one gallery ──────────────────────────────────

    // A picture is read by looking at it — a row per filename says nothing about
    // what is in the file — so the images are shown rather than listed, and what
    // becomes of them is stated once for the group. Clicking one opens the
    // lightbox at it.
    private Control ImagesRow(IReadOnlyList<BridgeMappingImage> images)
    {
        var grid = Grid();
        var gallery = new WrapPanel { Orientation = Orientation.Horizontal };
        foreach (var image in images)
        {
            var thumbnail = new Image();
            var tile = DialogUi.CoverTile(
                thumbnail,
                image.Name,
                image.IsCover ? Loc.Core("ui.import.becomes.cover") : string.Empty);
            var path = image.LocalPath;
            tile.Click += (_, _) => _actions.OpenImages(images, path);
            gallery.Children.Add(tile);
            _loadImage(thumbnail, path);
        }
        Avalonia.Controls.Grid.SetColumn(gallery, 0);
        grid.Children.Add(gallery);
        AddBecomesText(grid, Loc.Core("ui.import.becomes.kept"));
        return Host(grid, audioPath: null);
    }

    // ── A directory whose files all do the same job ──────────────────────────

    // As the one row core decided it should be: the prefix, what it holds, and
    // the total size.
    private Control DirectoryRow(BridgeCollapsedDirectory directory)
    {
        var grid = Grid();
        var kindLine = Loc.Core(
            BaeBridgeMethods.BridgeFileRowKindKey(directory.Kind), "count", (long)directory.Count);
        var name = ImportPaneUi.FileName(
            directory.DirPrefix, $"— {kindLine}", checked((long)directory.TotalSize));
        Avalonia.Controls.Grid.SetColumn(name, 0);
        grid.Children.Add(name);
        AddBecomesText(grid, Loc.Core("ui.import.becomes.kept"));
        return Host(grid, audioPath: null);
    }
}
