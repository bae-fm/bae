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
/// The mapping pane's second zone: every file in the folder exactly once, with
/// the job the scan proposed for it, what that job makes of it, and the control
/// that changes it.
///
/// A homogeneous directory core marked as collapsed is one group row standing
/// in for all of its files. A track sheet carries its binding here — the
/// sheet↔audio picker that used to sit beside the import picker's document
/// list, which is where it lived only until this pane existed.
/// </summary>
internal sealed class ImportRolesTable
{
    private readonly BridgeCandidateFiles _files;
    private readonly Func<string, BridgeFileRoleChoice, Task> _setRole;
    private readonly Func<string, Task<List<ImportSheetBindingOption>>> _bindingOptions;
    private readonly Func<string, string?, Task> _setBinding;
    private readonly Action<ImportDocument> _openDocument;

    internal ImportRolesTable(
        BridgeCandidateFiles files,
        Func<string, BridgeFileRoleChoice, Task> setRole,
        Func<string, Task<List<ImportSheetBindingOption>>> bindingOptions,
        Func<string, string?, Task> setBinding,
        Action<ImportDocument> openDocument)
    {
        _files = files;
        _setRole = setRole;
        _bindingOptions = bindingOptions;
        _setBinding = setBinding;
        _openDocument = openDocument;
    }

    internal Control Build()
    {
        var column = new StackPanel { Spacing = 0 };
        column.Children.Add(ImportPaneUi.ZoneTitle(Loc.Core("ui.import.roles.title"), _files.FormatLabel));
        column.Children.Add(HeaderRow());

        var prefixes = _files.Files.Select(file => file.File.DirPrefix).ToList();
        foreach (var row in MappingPaneModel.FileRows(prefixes, MappingPaneProjection.CollapsedDirectories(_files)))
        {
            column.Children.Add(row.Directory is { } directory
                ? GroupRow(directory)
                : FileRow(_files.Files[row.FileIndex!.Value]));
        }
        return column;
    }

    private static Control HeaderRow()
    {
        var grid = Grid();
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.slots.column.file"), 0));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.roles.column.role"), 1));
        grid.Children.Add(ImportPaneUi.ColumnHeader(Loc.Core("ui.import.roles.column.becomes"), 2));
        grid.Margin = new Thickness(0, 0, 0, 4);
        return grid;
    }

    private static Grid Grid() => new()
    {
        ColumnDefinitions = new ColumnDefinitions("*,150,150,Auto"),
        ColumnSpacing = 10,
    };

    // One file: its name (with the directory prefix dimmed ahead of it) and
    // size, the role in force, what that role makes of it, and — when the role
    // is somebody's decision — the control that changes it.
    private Control FileRow(BridgeCandidateFile file)
    {
        var grid = Grid();

        var name = ImportPaneUi.FileName(file.File.DirPrefix, file.File.FileName, checked((long)file.File.Size));
        Avalonia.Controls.Grid.SetColumn(name, 0);
        grid.Children.Add(name);

        var role = new StackPanel { Spacing = 4 };
        role.Children.Add(ImportPaneUi.Cell(Loc.Core(BaeBridgeMethods.BridgeFileRoleKey(file.Role))));
        if (file.Role is BridgeFileRole.TrackSheet sheet)
        {
            role.Children.Add(SheetBindingControl(file.File.Name, sheet));
        }
        Avalonia.Controls.Grid.SetColumn(role, 1);
        grid.Children.Add(role);

        var becomes = ImportPaneUi.Cell(BecomesLine(file.Becomes), secondary: true);
        Avalonia.Controls.Grid.SetColumn(becomes, 2);
        grid.Children.Add(becomes);

        if (RoleControl(file) is { } control)
        {
            Avalonia.Controls.Grid.SetColumn(control, 3);
            grid.Children.Add(control);
        }

        var host = new Border { Padding = new Thickness(0, 5), Child = grid, Background = Brushes.Transparent };
        if (file.Role is BridgeFileRole.Document or BridgeFileRole.TrackSheet)
        {
            // A readable file opens in the document viewer, which is the only
            // thing there is to do with one — the affordance the picker's
            // document list carried before this pane replaced it.
            var document = new ImportDocument
            {
                Name = file.File.Name,
                Path = file.File.LocalPath,
                SizeBytes = checked((long)file.File.Size),
            };
            host.Cursor = new Avalonia.Input.Cursor(Avalonia.Input.StandardCursorType.Hand);
            host.DoubleTapped += (_, _) => _openDocument(document);
        }
        return host;
    }

    // A directory whose files all do the same job, as core decided: one row for
    // all of them, reading "covers/ — 14 images" with their total size.
    private static Control GroupRow(MappingCollapsedDirectory directory)
    {
        var grid = Grid();

        var kindLine = Loc.Core(
            BaeBridgeMethods.BridgeFileRowKindKey(MappingPaneProjection.RowKind(directory.Kind)),
            "count",
            (long)directory.Count);
        var name = ImportPaneUi.FileName(
            directory.DirPrefix, $"— {kindLine}", checked((long)directory.TotalSize));
        Avalonia.Controls.Grid.SetColumn(name, 0);
        grid.Children.Add(name);

        return new Border { Padding = new Thickness(0, 5), Child = grid };
    }

    private static string BecomesLine(BridgeFileBecomes becomes) => becomes switch
    {
        BridgeFileBecomes.Slots slots when slots.First == slots.Last =>
            Loc.Core(BaeBridgeMethods.BridgeFileBecomesKey(becomes), "slot", (long)slots.First),
        BridgeFileBecomes.Slots slots => Loc.Core(
            BaeBridgeMethods.BridgeFileBecomesKey(becomes),
            new Dictionary<string, object?> { ["first"] = (long)slots.First, ["last"] = (long)slots.Last }),
        _ => Loc.Core(BaeBridgeMethods.BridgeFileBecomesKey(becomes)),
    };

    // The control that puts a file in a role. Present only where core offered
    // alternatives, which is every file the scan read as audio and nothing
    // else: an image is an image, and a track sheet's job is decided by what it
    // is bound to. A file already out of the tracklist gets the shorthand
    // action instead of a two-item menu.
    private Control? RoleControl(BridgeCandidateFile file)
    {
        if (file.Alternatives.Length == 0)
        {
            return null;
        }
        if (file.RoleChoice == BridgeFileRoleChoice.NotATrack)
        {
            var putBack = ImportPaneUi.RowButton(Loc.Core("ui.import.slots.put_back"));
            putBack.Click += async (_, _) => await _setRole(file.File.Name, BridgeFileRoleChoice.Audio);
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
            item.Click += async (_, _) => await _setRole(file.File.Name, choice);
            return (Control)item;
        }).ToList();
        button.Flyout = new MenuFlyout { ItemsSource = items };
        return button;
    }

    // What a track sheet describes, and the picker that names it. The choices
    // come from core already filtered to what the sheet can use, each refusal
    // carrying its reason — offering a file the commit would reject is the
    // failure the editable binding exists to remove.
    private Control SheetBindingControl(string sheetFileId, BridgeFileRole.TrackSheet sheet)
    {
        var combo = new ComboBox
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            FontSize = 12,
            PlaceholderText = Loc.Core("ui.import.sheet.choose_audio"),
        };
        // Repopulating sets SelectedItem, which raises SelectionChanged; without
        // this the initial fill would read as the user picking what is already
        // bound and write it back.
        var filling = false;
        var bound = sheet.Binding switch
        {
            BridgeSheetBinding.Describes describes => describes.FileId,
            BridgeSheetBinding.RefusedCodec refused => refused.FileId,
            _ => null,
        };
        combo.SelectionChanged += async (_, _) =>
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
            await _setBinding(sheetFileId, audioFileId);
        };

        var column = new StackPanel { Spacing = 3 };
        if (BridgeDisplay.UnboundSheetLine(sheet.Binding) is { Length: > 0 } unbound)
        {
            column.Children.Add(ImportPaneUi.Cell(unbound, secondary: true));
        }
        if (sheet.Binding is BridgeSheetBinding.Unresolved { Requested.Length: > 0 } unresolved)
        {
            column.Children.Add(ImportPaneUi.Cell(
                Loc.Core("ui.import.sheet.asked_for", "names", string.Join(", ", unresolved.Requested)),
                secondary: true));
        }
        column.Children.Add(combo);

        // Core probes every audio file to answer, so the choices are read once,
        // when the row is built.
        _ = FillOptions(combo, sheetFileId, bound, () => filling = true, () => filling = false);
        return column;
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
}
