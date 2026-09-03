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

internal sealed partial class ImportMappingPane
{
    private Control BuildReadOnlyImportPane(
        BridgeTriageImportStatus.Complete? completed)
    {
        var sections = new StackPanel
        {
            Spacing = 18,
            Margin = new Thickness(20, 16, 20, 16),
        };
        sections.Children.Add(FolderLine());
        sections.Children.Add(ReadOnlyStatusLine(completed));
        sections.Children.Add(ReadOnlyMetadataSummary());
        if (_candidate!.Mapping.Images.Length > 0)
        {
            sections.Children.Add(new ImportMappingGallery(
                _candidate.Mapping.Images,
                _candidate.FileEvidence,
                (image, path) => _app.Images.Bind(
                    image,
                    new ImageContent.LocalFile(path),
                    ImageWidths.PickerTile),
                ShowFolderImages).Build());
        }
        _table = null;
        sections.Children.Add(new ReadOnlyImportMappingTable(
            _candidate.Mapping,
            _candidate.Edit?.AlbumArtistAssignments
                ?? Array.Empty<BridgeArtistAssignment>(),
            () => _import.PreviewingTarget,
            (name, path) => _ = _dialogs.ShowDocumentFile(
                new ImportDocument { Name = name, Path = path }),
            target => _app.Playback.PreviewPlay(target),
            () => _app.Playback.PreviewStop()).Build());
        return new ScrollViewer { Content = sections };
    }

    private Control ReadOnlyStatusLine(
        BridgeTriageImportStatus.Complete? completed)
    {
        if (completed is null)
        {
            return ImportProgressLine.Build(_import, _key!);
        }
        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 10,
        };
        row.Children.Add(ImportPaneUi.Cell(Loc.Chrome("import.complete")));
        var view = ImportPaneUi.RowButton(Loc.Chrome("import.view_in_library"));
        view.Click += async (_, _) => await _dialogs.OpenAlbum(completed.AlbumId);
        row.Children.Add(view);
        return row;
    }

    private Control ReadOnlyMetadataSummary()
    {
        var edit = _candidate!.Edit;
        var text = new StackPanel { Spacing = 5 };
        var title = new TextBlock
        {
            Text = edit?.AlbumTitle ?? MetadataTitle(),
            FontSize = 22,
            FontWeight = FontWeight.SemiBold,
        };
        title[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextPrimaryBrush");
        text.Children.Add(title);
        if (edit is not null)
        {
            text.Children.Add(ImportPaneUi.Cell(
                ArtistAssignmentDisplay.Join(edit.AlbumArtistAssignments),
                secondary: true));
            var facts = new[] { edit.AlbumYear, MetaLine() }
                .Where(value => !string.IsNullOrWhiteSpace(value));
            text.Children.Add(ImportPaneUi.Cell(
                string.Join("  ·  ", facts), secondary: true));
        }
        var audio = SourceAudioLine(_candidate.Files);
        if (audio.Length > 0)
        {
            text.Children.Add(ImportPaneUi.Cell(audio, secondary: true));
        }

        var cover = new Image
        {
            Width = 180,
            Height = 180,
            Stretch = Avalonia.Media.Stretch.UniformToFill,
        };
        if (_candidate.Cover is { } choice)
        {
            _app.Images.Bind(
                cover,
                ImportDialogs.CoverChoiceContent(choice),
                ImageWidths.PickerTile);
        }
        var coverHost = new Border
        {
            Width = 180,
            Height = 180,
            CornerRadius = new CornerRadius(8),
            ClipToBounds = true,
            Child = cover,
        };
        coverHost[!Border.BackgroundProperty] =
            new DynamicResourceExtension("BaeElevatedBrush");
        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 20,
            Children = { coverHost, text },
        };
        return row;
    }
}

internal sealed class ReadOnlyImportMappingTable(
    BridgeMappingTable table,
    IReadOnlyList<BridgeArtistAssignment> albumArtists,
    Func<BridgePreviewTarget?> previewingTarget,
    Action<string, string> openDocument,
    Action<BridgePreviewTarget> preview,
    Action stopPreview)
{
    private readonly List<Grid> _grids = new();

    internal Control Build()
    {
        var sections = new StackPanel { Spacing = 18 };
        sections.Children.Add(Section(
            Loc.Core("ui.import.mapping.tracks_title"),
            TrackHeader(),
            TrackRows()));
        var files = FileRows();
        if (files.Count > 0)
        {
            sections.Children.Add(Section(
                Loc.Core("ui.import.mapping.files_title"),
                FileHeader(),
                files));
        }
        return sections;
    }

    private List<Control> TrackRows()
    {
        var rows = new List<Control>();
        foreach (var section in table.TrackSections)
        {
            if (section.HeaderText() is { } heading)
            {
                rows.Add(SectionHeading(heading));
            }
            if (section.Content is BridgeMappingTrackSectionContent.Sheet sheet)
            {
                rows.Add(SheetCaption(sheet.SheetValue));
            }
            foreach (var mapping in section.Mappings())
            {
                rows.Add(TrackRow(mapping));
            }
        }
        return rows;
    }

    private Control TrackRow(BridgeTrackMapping mapping)
    {
        var grid = RowGrid();
        if (mapping.Becomes is BridgeMappingBecomes.Track becomes)
        {
            var track = becomes.TrackValue;
            AddCell(grid, ImportPaneUi.Cell(becomes.Position, secondary: true), 0);
            AddCell(grid, ImportPaneUi.Cell(track.Title), 1);
            AddCell(grid, ImportPaneUi.Cell(TrackArtists(track)), 2);
        }
        else
        {
            AddCell(grid, ImportPaneUi.Cell(string.Empty), 0);
            AddCell(
                grid,
                ImportPaneUi.Cell(
                    Loc.Core("ui.import.becomes.awaiting_pick"),
                    secondary: true),
                1);
            AddCell(grid, ImportPaneUi.Cell(string.Empty), 2);
        }
        var duration = ImportPaneUi.Cell(
            MappingTableReading.DurationText(mapping.DurationMs));
        duration.HorizontalAlignment = HorizontalAlignment.Right;
        AddCell(grid, duration, 3);
        AddCell(grid, SourceCell(mapping.Source), 4);
        return RowHost(grid);
    }

    private string TrackArtists(BridgeRawTrackEdit track) =>
        track.ArtistAssignments switch
        {
            BridgeTrackArtistAssignments.AlbumArtists =>
                ArtistAssignmentDisplay.Join(albumArtists),
            BridgeTrackArtistAssignments.Explicit explicitArtists =>
                ArtistAssignmentDisplay.Join(explicitArtists.Assignments),
            _ => throw new ArgumentOutOfRangeException(
                nameof(track), track.ArtistAssignments,
                "Unknown track artist assignments"),
        };

    private Control SourceCell(BridgeMappingSource source)
    {
        var line = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
        };
        if (source.PreviewTarget() is { } target)
        {
            var playing = previewingTarget() == target;
            var button = ImportPaneUi.RowButton(Loc.Core(
                playing ? "ui.import.slots.stop" : "ui.import.slots.play"));
            button.Click += (_, _) =>
            {
                if (previewingTarget() == target)
                {
                    stopPreview();
                }
                else
                {
                    preview(target);
                }
            };
            line.Children.Add(button);
        }
        var label = source switch
        {
            BridgeMappingSource.File file => file.FileValue.Name,
            BridgeMappingSource.SheetEntry entry =>
                entry.Entry.Title ?? entry.Entry.ContainerName,
            BridgeMappingSource.Missing => Loc.Core("ui.import.slots.no_file"),
            _ => throw new ArgumentOutOfRangeException(
                nameof(source), source, "Unknown mapping source"),
        };
        var name = ImportPaneUi.Cell(label, secondary: true);
        name.FontFamily = new FontFamily("monospace");
        line.Children.Add(name);
        return line;
    }

    private List<Control> FileRows()
    {
        var rows = new List<Control>();
        foreach (var mapping in table.Files)
        {
            var grid = RowGrid();
            switch (mapping)
            {
                case BridgeMappingFileRow.File fileRow:
                    var file = fileRow.FileValue;
                    Control name = ImportPaneUi.Cell(file.Name);
                    if (file.Role is BridgeMappingRole.Document)
                    {
                        var documentButton = ImportPaneUi.RowButton(file.Name);
                        documentButton.Click += (_, _) => openDocument(
                            file.Name, file.LocalPath);
                        name = documentButton;
                    }
                    AddCell(grid, name, 0, span: 3);
                    var size = ImportPaneUi.Cell(
                        Loc.Bytes(checked((long)file.Size)), secondary: true);
                    size.HorizontalAlignment = HorizontalAlignment.Right;
                    AddCell(grid, size, 3);
                    AddCell(
                        grid,
                        ImportPaneUi.Cell(
                            Loc.Core(BaeBridgeMethods.BridgeFileRoleKey(
                                file.Role.FileRole())),
                            secondary: true),
                        4);
                    break;
                case BridgeMappingFileRow.Sheet sheetRow:
                    var sheet = sheetRow.SheetValue;
                    var sheetButton = ImportPaneUi.RowButton(sheet.Name);
                    sheetButton.Click += (_, _) => openDocument(
                        sheet.Name, sheet.LocalPath);
                    AddCell(grid, sheetButton, 0, span: 3);
                    var sheetSize = ImportPaneUi.Cell(
                        Loc.Bytes(checked((long)sheet.Size)), secondary: true);
                    sheetSize.HorizontalAlignment = HorizontalAlignment.Right;
                    AddCell(grid, sheetSize, 3);
                    AddCell(
                        grid,
                        ImportPaneUi.Cell(DiscLabel(sheet.Assignment), secondary: true),
                        4);
                    break;
            }
            rows.Add(RowHost(grid));
        }
        return rows;
    }

    private Control TrackHeader()
    {
        var grid = RowGrid();
        AddCell(grid, ImportPaneUi.ColumnHeader(
            Loc.Chrome("edit.tracks.col_number"), 0), 0);
        AddCell(grid, ImportPaneUi.ColumnHeader(
            Loc.Core("ui.import.mapping.column.title"), 0), 1);
        AddCell(grid, ImportPaneUi.ColumnHeader(
            Loc.Core("ui.import.mapping.column.artist"), 0), 2);
        AddCell(grid, ImportPaneUi.ColumnHeader(
            Loc.Core("ui.import.slots.column.length"), 0), 3);
        AddCell(grid, ImportPaneUi.ColumnHeader(
            Loc.Core("ui.import.mapping.column.source"), 0), 4);
        return grid;
    }

    private Control FileHeader()
    {
        var grid = RowGrid();
        AddCell(grid, ImportPaneUi.ColumnHeader(
            Loc.Core("ui.import.mapping.column.name"), 0), 0, span: 3);
        AddCell(grid, ImportPaneUi.ColumnHeader(
            Loc.Chrome("storage.column.size"), 0), 3);
        AddCell(grid, ImportPaneUi.ColumnHeader(
            Loc.Core("ui.import.roles.column.role"), 0), 4);
        return grid;
    }

    private Control Section(string title, Control header, List<Control> rows)
    {
        var column = new StackPanel
        {
            Spacing = 0,
            MinWidth = ImportMappingColumns.MinimumWidth,
        };
        column.Children.Add(header);
        foreach (var row in rows)
        {
            column.Children.Add(row);
        }
        var scroller = new ScrollViewer
        {
            Content = column,
            HorizontalScrollBarVisibility =
                Avalonia.Controls.Primitives.ScrollBarVisibility.Auto,
            VerticalScrollBarVisibility =
                Avalonia.Controls.Primitives.ScrollBarVisibility.Disabled,
        };
        scroller.SizeChanged += (_, e) => ApplyColumns(e.NewSize.Width);
        var section = new StackPanel { Spacing = 8 };
        section.Children.Add(ImportPaneUi.ZoneTitle(title, null));
        section.Children.Add(scroller);
        return section;
    }

    private Grid RowGrid()
    {
        var widths = ImportMappingColumns.Resolve(
            ImportMappingColumns.MinimumWidth);
        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions
            {
                new(new GridLength(ImportMappingColumns.Position)),
                new(new GridLength(widths.Title)),
                new(new GridLength(widths.Artist)),
                new(new GridLength(ImportMappingColumns.Length)),
                new(new GridLength(widths.Source)),
            },
            ColumnSpacing = ImportMappingColumns.Spacing,
        };
        _grids.Add(grid);
        return grid;
    }

    private void ApplyColumns(double width)
    {
        var columns = ImportMappingColumns.Resolve(width);
        foreach (var grid in _grids)
        {
            grid.ColumnDefinitions[1].Width = new GridLength(columns.Title);
            grid.ColumnDefinitions[2].Width = new GridLength(columns.Artist);
            grid.ColumnDefinitions[4].Width = new GridLength(columns.Source);
        }
    }

    private static void AddCell(
        Grid grid, Control control, int column, int span = 1)
    {
        Grid.SetColumn(control, column);
        Grid.SetColumnSpan(control, span);
        grid.Children.Add(control);
    }

    private static Border RowHost(Grid row)
    {
        var host = new Border
        {
            Padding = new Thickness(0, 8),
            BorderThickness = new Thickness(0, 1, 0, 0),
            Child = row,
        };
        host[!Border.BorderBrushProperty] =
            new DynamicResourceExtension("BaeHairlineBrush");
        return host;
    }

    private static Control SectionHeading(string text)
    {
        var heading = ImportPaneUi.Cell(text.ToUpper(
            System.Globalization.CultureInfo.CurrentUICulture), secondary: true);
        heading.FontWeight = FontWeight.Bold;
        return new Border
        {
            Padding = new Thickness(0, 12, 0, 5),
            Child = heading,
        };
    }

    private static Control SheetCaption(BridgeSheetGroup sheet) =>
        new Border
        {
            Padding = new Thickness(0, 8),
            Child = ImportPaneUi.Cell(
                $"{sheet.Name}  ·  {DiscLabel(sheet.Assignment)}",
                secondary: true),
        };

    private static string DiscLabel(BridgeSheetDisc assignment) => assignment switch
    {
        BridgeSheetDisc.Disc disc => Loc.Core(
            "ui.import.sheet.disc", "number", (long)disc.Number),
        BridgeSheetDisc.Ignored => Loc.Core("ui.import.sheet.ignored"),
        _ => throw new ArgumentOutOfRangeException(
            nameof(assignment), assignment, "Unknown sheet disc"),
    };
}
