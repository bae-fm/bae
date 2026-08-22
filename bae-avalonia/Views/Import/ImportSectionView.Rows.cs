using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using Avalonia.Platform.Storage;
using uniffi.bae_bridge;

namespace Bae.Desktop;

internal sealed partial class ImportSectionView
{
    private ScrollViewer RenderReleaseSections(BridgeTriageTab tab)
    {
        var list = new StackPanel { Spacing = 0 };
        foreach (var section in TriageListModel.Sections(
            _import.TriageQueue,
            tab,
            _import.FilterText,
            _import.SortOrder))
        {
            var content = new StackPanel { Spacing = 0 };
            foreach (var entry in section.Entries)
            {
                var control = entry.Bridge switch
                {
                    BridgeTriageEntry.Candidate candidate => BuildRow(candidate.Row),
                    BridgeTriageEntry.Boundary boundary => BuildBoundaryRow(boundary.BoundaryValue),
                    BridgeTriageEntry.Invalid invalid => BuildInvalidRow(invalid.InvalidCandidate),
                    _ => new Panel(),
                };
                control.Tag = entry.StableKey;
                content.Children.Add(control);
            }
            if (section.Group is { } group)
            {
                var expanded = _import.Interaction.IsGroupExpanded(
                    ImportStore.GroupDisclosureKey(group.Key));
                list.Children.Add(BuildGroupHeader(group, expanded));
                if (expanded)
                {
                    list.Children.Add(content);
                }
            }
            else
            {
                list.Children.Add(content);
            }
        }
        return new ScrollViewer { Content = list };
    }

    // A folder group's header row: the disclosure caret, a folder glyph, and
    // the folder's name. Its rows follow as siblings rather than as an
    // Expander's content, which would indent them — grouped and ungrouped rows
    // share one leading edge, and this row is the only thing that says a group
    // is there.
    private Control BuildGroupHeader(BridgeTriageGroup group, bool expanded)
    {
        var caret = Icons.Glyph(expanded ? Icons.ChevronDown : Icons.ChevronRight, 12, "BaeTextSecondaryBrush");
        var folder = Icons.Glyph(Icons.Folder, 13, "BaeTextSecondaryBrush");
        var name = new TextBlock
        {
            Text = group.Name,
            FontSize = 12.5,
            FontWeight = FontWeight.SemiBold,
            VerticalAlignment = VerticalAlignment.Center,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        name[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var content = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 5,
            Margin = new Thickness(9, 6, 10, 6),
            Children = { caret, folder, name },
        };

        var button = new Button
        {
            Content = content,
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(0),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
        };
        button.Click += (_, _) =>
        {
            _import.Interaction.SetGroupExpanded(
                ImportStore.GroupDisclosureKey(group.Key),
                !expanded);
            Render();
        };

        var combine = new MenuItem { Header = Loc.Chrome("import.release.one") };
        combine.Click += (_, _) => ApplyFolderReleaseDecision(
            group.Key,
            BridgeFolderReleaseDecision.CombineAsOneRelease);
        var separate = new MenuItem { Header = Loc.Chrome("import.release.separate") };
        separate.Click += (_, _) => ApplyFolderReleaseDecision(
            group.Key,
            BridgeFolderReleaseDecision.KeepAsSeparateReleases);
        button.ContextFlyout = new MenuFlyout { ItemsSource = new[] { combine, separate } };
        return button;
    }

    private Control BuildBoundaryRow(BridgeFolderReleaseBoundary boundary)
    {
        var content = new StackPanel { Spacing = 5, Margin = new Thickness(14, 9) };
        var title = new TextBlock
        {
            Text = boundary.Name,
            FontSize = 14,
            FontWeight = FontWeight.SemiBold,
        };
        var tree = new StackPanel { Spacing = 3 };
        var displayPath = new TextBlock
        {
            Text = boundary.DisplayPath,
            FontFamily = new FontFamily("monospace"),
            FontSize = 11.5,
        };
        displayPath[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        tree.Children.Add(displayPath);
        if (boundary.SharedFileCount > 0)
        {
            var sharedFiles = new TextBlock
            {
                Text = Loc.Chrome("storage.files", "count", (long)boundary.SharedFileCount),
                FontSize = 11.5,
            };
            sharedFiles[!TextBlock.ForegroundProperty] =
                new DynamicResourceExtension("BaeTextSecondaryBrush");
            tree.Children.Add(sharedFiles);
        }
        foreach (var row in boundary.TreeRows)
        {
            var detail = row.Kind switch
            {
                BridgeFolderReleaseTreeRowKind.Candidate candidate =>
                    $"{candidate.TrackCount.ToString(CultureInfo.CurrentCulture)} · {candidate.FormatLabel}",
                BridgeFolderReleaseTreeRowKind.Invalid invalid =>
                    BridgeDisplay.LocalizedLine(invalid.Reason),
                _ => null,
            };
            var text = new TextBlock
            {
                Text = detail is null ? row.Name : $"{row.Name}  {detail}",
                FontSize = 11.5,
                Margin = new Thickness(row.Depth * 12, 0, 0, 0),
            };
            text[!TextBlock.ForegroundProperty] =
                new DynamicResourceExtension(
                    row.Kind is BridgeFolderReleaseTreeRowKind.Invalid
                        ? "BaeDangerBrush"
                        : "BaeTextSecondaryBrush");
            text.ContextMenu = BuildDecisionMenu(row.DecisionKey);
            tree.Children.Add(text);
        }
        content.Children.Insert(0, title);
        content.Children.Insert(1, tree);
        var one = new Button
        {
            Content = Loc.Chrome("import.release.one"),
            Padding = new Thickness(10, 4),
        };
        one.Click += (_, _) => ApplyFolderReleaseDecision(
            boundary.Key,
            BridgeFolderReleaseDecision.CombineAsOneRelease);
        var separate = new Button
        {
            Content = Loc.Chrome("import.release.separate"),
            Padding = new Thickness(10, 4),
        };
        separate.Click += (_, _) => ApplyFolderReleaseDecision(
            boundary.Key,
            BridgeFolderReleaseDecision.KeepAsSeparateReleases);
        var actions = new StackPanel
        {
            Orientation = Orientation.Vertical,
            Spacing = 6,
            Children = { one, separate },
        };
        one.HorizontalAlignment = HorizontalAlignment.Stretch;
        separate.HorizontalAlignment = HorizontalAlignment.Stretch;
        content.Children.Add(actions);
        return new Border { Child = content };
    }

    private ContextMenu BuildDecisionMenu(BridgeFolderReleaseDecisionKey key)
    {
        var combine = new MenuItem
        {
            Header = Loc.Chrome("import.release.one"),
        };
        combine.Click += (_, _) => ApplyFolderReleaseDecision(
            key,
            BridgeFolderReleaseDecision.CombineAsOneRelease);
        var separate = new MenuItem
        {
            Header = Loc.Chrome("import.release.separate"),
        };
        separate.Click += (_, _) => ApplyFolderReleaseDecision(
            key,
            BridgeFolderReleaseDecision.KeepAsSeparateReleases);
        return new ContextMenu { ItemsSource = new[] { combine, separate } };
    }

    // ── Rows ──────────────────────────────────────────────────────────────────

    // One triage row: an optional bulk-select checkbox, the matched release's
    // cover, its title/metadata, and trailing evidence or status. Every
    // decision about what the row shows — which tab, which group, whether it
    // takes a checkbox — is `row`'s, read off BridgeTriageRow; this only
    // renders it.
    private Control BuildRow(BridgeTriageRow row)
    {
        var grid = new Grid { ColumnDefinitions = new ColumnDefinitions("26,44,*,Auto"), Margin = new Thickness(9, 0, 0, 0) };

        var checkboxSlot = new Panel { Width = 26, Height = 18, VerticalAlignment = VerticalAlignment.Top, Margin = new Thickness(0, 7, 0, 0) };
        if (row.Selectable)
        {
            checkboxSlot.Children.Add(BuildCheckbox(row.CandidateKey));
        }
        Grid.SetColumn(checkboxSlot, 0);
        grid.Children.Add(checkboxSlot);

        var cover = BuildCover(row.Matched?.CoverThumbnailUrl);
        cover.Margin = new Thickness(0, 7, 10, 7);
        Grid.SetColumn(cover, 1);
        grid.Children.Add(cover);

        var text = BuildRowText(row);
        text.Margin = new Thickness(0, 7, 8, 7);
        Grid.SetColumn(text, 2);
        grid.Children.Add(text);

        var trailing = BuildRowTrailing(row);
        trailing.Margin = new Thickness(0, 9, 10, 7);
        trailing.VerticalAlignment = VerticalAlignment.Top;
        Grid.SetColumn(trailing, 3);
        grid.Children.Add(trailing);

        var isPending = row.Placement is BridgeTriagePlacement.NeedsYou { Reason: BridgeNeedsYouReason.StillIdentifying };
        var host = new Border
        {
            Child = grid,
            Opacity = isPending || !row.Actionable ? 0.6 : 1,
            Background = Brushes.Transparent,
            IsEnabled = row.Actionable,
        };
        if (row.CandidateKey == _selectedKey)
        {
            host[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeSelectionTintBrush");
        }
        ToolTip.SetTip(host, row.DisplayPath);
        host.Tapped += (_, _) => OnRowActivated(row);
        host.ContextMenu = BuildRowContextMenu(row);
        return host;
    }

    private Control BuildCheckbox(string key)
    {
        var isSelected = _import.SelectedReady.Contains(key);
        var box = new Border
        {
            Width = 18,
            Height = 18,
            CornerRadius = new CornerRadius(5),
            BorderThickness = new Thickness(isSelected ? 0 : 1.5),
        };
        if (isSelected)
        {
            box[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeAccentBrush");
        }
        else
        {
            box.Background = Brushes.Transparent;
        }
        box[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        if (isSelected)
        {
            var check = new TextBlock { Text = "✓", FontSize = 11, FontWeight = FontWeight.Bold, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
            check[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeOnAccentBrush");
            box.Child = check;
        }
        var button = new Button
        {
            Content = box,
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(0),
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        button.Click += (_, _) => _import.ToggleReadySelection(key);
        button.Tapped += (_, eventArgs) => eventArgs.Handled = true;
        return button;
    }

    private Control BuildCover(string? coverThumbnailUrl)
    {
        var image = new Image { Width = 44, Height = 44, Stretch = Stretch.UniformToFill };
        var host = new Border { Width = 44, Height = 44, CornerRadius = new CornerRadius(8), ClipToBounds = true, Child = image };
        host[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        if (!string.IsNullOrEmpty(coverThumbnailUrl))
        {
            _app.Images.Bind(image, new ImageContent.Remote(coverThumbnailUrl), ImageWidths.Row);
        }
        return host;
    }

    private Control BuildRowText(BridgeTriageRow row)
    {
        var upload = UploadProgressPresentation.ResolveImport(
            row.ImportStatus,
            _storage.Outbox);
        var column = new StackPanel { Spacing = 0 };
        var title = new TextBlock
        {
            Text = TriageListModel.DisplayTitle(row),
            FontSize = 14,
            FontWeight = FontWeight.SemiBold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
        };
        title[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        if (TriageListModel.TitleIsFolderName(row))
        {
            var titleRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 5 };
            titleRow.Children.Add(Icons.Glyph(Icons.Folder, 13, "BaeTextSecondaryBrush"));
            titleRow.Children.Add(title);
            column.Children.Add(titleRow);
        }
        else
        {
            column.Children.Add(title);
        }

        if (RowSubLine(row, upload) is { Length: > 0 } subLine)
        {
            var sub = new TextBlock
            {
                Text = subLine,
                FontSize = 12.5,
                MaxLines = 1,
                TextTrimming = TextTrimming.CharacterEllipsis,
                Margin = new Thickness(0, 1, 0, 0),
            };
            sub[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            column.Children.Add(sub);
        }

        if (row.ImportStatus is BridgeTriageImportStatus.Importing)
        {
            var bar = ThinProgressBar(ImportingProgress(row).Percent / 100.0);
            bar.Margin = new Thickness(0, 7, 0, 0);
            column.Children.Add(bar);
        }
        else if (upload is ImportUploadObservation.Awaiting
                 or ImportUploadObservation.Active)
        {
            var bar = CloudProgressBar(upload);
            bar.Margin = new Thickness(0, 7, 0, 0);
            column.Children.Add(bar);
        }

        var actions = BuildRowActions(row);
        if (actions is not null)
        {
            actions.Margin = new Thickness(0, 7, 0, 0);
            column.Children.Add(actions);
        }

        return column;
    }

    // The second line: the matched release's metadata, a disagreement sentence,
    // the still-identifying phase, or an import failure — whichever `row` is
    // actually saying.
    private string? RowSubLine(
        BridgeTriageRow row,
        ImportUploadObservation? upload) => row.Placement switch
        {
            BridgeTriagePlacement.Ready or BridgeTriagePlacement.Skipped => MetadataLine(row),
            BridgeTriagePlacement.NeedsYou { Reason: BridgeNeedsYouReason.StillIdentifying phase } =>
                BridgeDisplay.LocalizedLine(phase.Phase),
            BridgeTriagePlacement.NeedsYou(BridgeNeedsYouGroup.AlreadyInLibrary, BridgeNeedsYouReason.Disagreement) =>
                MetadataLine(row),
            BridgeTriagePlacement.NeedsYou(BridgeNeedsYouGroup.PickAPressing, BridgeNeedsYouReason.Disagreement) =>
                row.Matched?.Artist,
            BridgeTriagePlacement.NeedsYou { Reason: BridgeNeedsYouReason.Disagreement disagreement } =>
                BridgeDisplay.LocalizedLine(disagreement.DisagreementValue),
            BridgeTriagePlacement.Importing or BridgeTriagePlacement.Done =>
                ImportSubLine(row, upload),
            _ => null,
        };

    private static string? MetadataLine(BridgeTriageRow row)
    {
        if (row.Matched is not { } matched)
        {
            return null;
        }
        var parts = new List<string>();
        if (!string.IsNullOrEmpty(matched.Artist))
        {
            parts.Add(matched.Artist!);
        }
        if (matched.Pressing is { } pressing)
        {
            if (pressing.Year is { } year)
            {
                parts.Add(year.ToString(CultureInfo.CurrentCulture));
            }
            if (!string.IsNullOrEmpty(pressing.Format))
            {
                parts.Add(pressing.Format!);
            }
            if (pressing.TrackCount is { } trackCount)
            {
                parts.Add(Loc.Chrome("import.candidate.tracks", "count", (long)trackCount));
            }
        }
        return parts.Count == 0 ? null : string.Join(" · ", parts);
    }

    /// <summary>The running import's percent and step, from the candidate's
    /// runtime: the row says that an import is running, the runtime says how
    /// far. A row placed as importing whose runtime has not reported yet is at
    /// the start with no step named.</summary>
    private (int Percent, ImportStep? Step) ImportingProgress(BridgeTriageRow row)
    {
        var status = _import.Candidate(row.CandidateKey)?.RowStatus;
        return status is { Kind: "importing" }
            ? (status.ProgressPercent, status.Step)
            : (0, null);
    }

    private string? ImportSubLine(
        BridgeTriageRow row,
        ImportUploadObservation? upload) => row.ImportStatus switch
        {
            BridgeTriageImportStatus.Importing => string.Join(
                " · ",
                new[]
                {
                ImportingProgress(row).Step is { } step ? step.LocalizedLabel : Loc.Chrome("import.progress.identifying"),
                (ImportingProgress(row).Percent / 100.0).ToString("P0", CultureInfo.CurrentCulture),
                }.Where(part => part.Length > 0)),
            BridgeTriageImportStatus.Complete
                or BridgeTriageImportStatus.CloudUploadQueued =>
                upload switch
                {
                    ImportUploadObservation.Awaiting =>
                        Loc.Core("core.queue.queued", "count", 1),
                    ImportUploadObservation.Active active => string.Join(
                        " · ",
                        new[]
                        {
                        UploadProgressPresentation.ActivityLabel(active.Progress),
                        UploadProgressPresentation.BarLabel(active.Progress.Bar),
                        }.Where(part => part.Length > 0)),
                    ImportUploadObservation.Finished => MetadataLine(row),
                    _ => throw new InvalidOperationException(
                        "a completed import has no upload observation"),
                },
            null => MetadataLine(row),
            BridgeTriageImportStatus.Error error => BridgeDisplay.LocalizedLine(error.ErrorValue),
            _ => null,
        };

    private static ProgressBar CloudProgressBar(
        ImportUploadObservation observation)
    {
        var fraction = observation switch
        {
            ImportUploadObservation.Awaiting => null,
            ImportUploadObservation.Active active =>
                UploadProgressPresentation.BarFraction(active.Progress.Bar),
            _ => throw new InvalidOperationException(
                "a finished import has no upload progress bar"),
        };
        return new ProgressBar
        {
            Height = 3,
            Minimum = 0,
            Maximum = 1,
            Value = fraction ?? 0,
            IsIndeterminate = fraction is null,
        };
    }

    // The pill row under the meta column — only the placements the design
    // gives one: pick-a-pressing, already-in-library, and a failed import.
    // Every other row's action is "activate it," which the whole row already
    // does.
    private Control? BuildRowActions(BridgeTriageRow row)
    {
        var pills = new List<(string Label, bool IsKey, Action Action)>();
        pills.AddRange(row.Placement switch
        {
            BridgeTriagePlacement.NeedsYou(BridgeNeedsYouGroup.AlreadyInLibrary, BridgeNeedsYouReason.Disagreement) =>
                new[]
                {
                    (Loc.Chrome("import.row.import_anyway"), false, (Action)(() => OnRowActivated(row))),
                },
            BridgeTriagePlacement.Done when row.ImportStatus is BridgeTriageImportStatus.Error =>
                new[]
                {
                    (Loc.Chrome("import.row.retry"), true, (Action)(() => OnRowActivated(row))),
                    (Loc.Chrome("libraries.reveal"), false, (Action)(() => RevealInFileManager.Reveal(row.CandidateKey))),
                },
            _ => Array.Empty<(string, bool, Action)>(),
        });
        foreach (var boundary in row.ResolvedBoundaries)
        {
            var decision = boundary.Decision is BridgeFolderReleaseDecision.CombineAsOneRelease
                ? BridgeFolderReleaseDecision.KeepAsSeparateReleases
                : BridgeFolderReleaseDecision.CombineAsOneRelease;
            pills.Add((
                Loc.Chrome(
                    decision is BridgeFolderReleaseDecision.CombineAsOneRelease
                        ? "import.release.one"
                        : "import.release.separate"),
                false,
                () => ApplyFolderReleaseDecision(boundary.Key, decision)));
        }
        if (pills.Count == 0)
        {
            return null;
        }
        var row2 = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        foreach (var (label, isKey, action) in pills)
        {
            row2.Children.Add(BuildActionPill(label, isKey, action));
        }
        return row2;
    }

    private static Button BuildActionPill(string label, bool isKey, Action onClick)
    {
        var button = new Button
        {
            Content = label,
            FontSize = 12,
            FontWeight = FontWeight.Medium,
            Padding = new Thickness(12, 4),
            CornerRadius = new CornerRadius(999),
            BorderThickness = new Thickness(isKey ? 0 : 1),
        };
        if (isKey)
        {
            button[!Button.BackgroundProperty] = new DynamicResourceExtension("BaeAccentBrush");
        }
        else
        {
            button.Background = Brushes.Transparent;
        }
        button[!Button.BorderBrushProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        button[!Button.ForegroundProperty] = new DynamicResourceExtension(isKey ? "BaeOnAccentBrush" : "BaeTextSecondaryBrush");
        button.Click += (_, _) => onClick();
        button.Tapped += (_, eventArgs) => eventArgs.Handled = true;
        return button;
    }

    private Control BuildRowTrailing(BridgeTriageRow row)
    {
        switch (row.Placement)
        {
            case BridgeTriagePlacement.Ready:
                return row.Matched is { } matched ? ReadyTrailing(matched.Evidence) : new Panel();
            case BridgeTriagePlacement.NeedsYou(var group, var reason):
                return NeedsYouTrailing(row, group, reason);
            case BridgeTriagePlacement.Importing:
                return new Spinner { Width = 14, Height = 14 };
            case BridgeTriagePlacement.Done:
                return DoneTrailing(row);
            default:
                return new Panel();
        }
    }

    private static Control ReadyTrailing(BridgeMatchEvidence evidence)
    {
        var chip = new Border
        {
            CornerRadius = new CornerRadius(4),
            Padding = new Thickness(5, 2),
            Child = new TextBlock
            {
                Text = ProviderShortCode(evidence.Source),
                FontFamily = new FontFamily("monospace"),
                FontSize = 10,
                FontWeight = FontWeight.Medium,
            },
        };
        chip[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        ((TextBlock)chip.Child!)[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var tip = evidence.Signal is { } signal
            ? $"{SignalLabel(signal)} · {ProviderDisplayName(evidence.Source)}"
            : ProviderDisplayName(evidence.Source);
        ToolTip.SetTip(chip, tip);
        return new StackPanel { HorizontalAlignment = HorizontalAlignment.Right, Children = { chip } };
    }

    private Control NeedsYouTrailing(BridgeTriageRow row, BridgeNeedsYouGroup group, BridgeNeedsYouReason reason)
    {
        switch (reason)
        {
            case BridgeNeedsYouReason.StillIdentifying stillIdentifying:
                // A run in flight spins; one waiting its turn, or one that
                // settled without an answer worth keeping, shows the clock —
                // the same two glyphs the macOS row uses for the same three
                // phases.
                return stillIdentifying.Phase == BridgeIdentifyPhase.Running
                    ? new Spinner { Width = 14, Height = 14 }
                    : Icons.Glyph(Icons.Clock, 14, "BaeTextSecondaryBrush");
            case BridgeNeedsYouReason.Disagreement disagreement:
                return group switch
                {
                    BridgeNeedsYouGroup.PickAPressing => Chip(BridgeDisplay.LocalizedLine(disagreement.DisagreementValue), "BaeWarningBrush"),
                    BridgeNeedsYouGroup.SignalsDisagree => Chip(Loc.Chrome("import.row.conflict"), "BaeDangerBrush"),
                    BridgeNeedsYouGroup.AlreadyInLibrary => Chip(BridgeDisplay.LocalizedLine(disagreement.DisagreementValue), "BaeInfoBrush"),
                    BridgeNeedsYouGroup.CountsOrLengthsDisagree => DotIcon("BaeWarningBrush"),
                    BridgeNeedsYouGroup.NoMatch => SearchManuallyChip(row),
                    _ => new Panel(),
                };
            default:
                return new Panel();
        }
    }

    private Control SearchManuallyChip(BridgeTriageRow row)
    {
        var button = BuildActionPill(Loc.Chrome("import.row.search_manually"), isKey: false, () => OnRowActivated(row));
        button.Padding = new Thickness(7, 3);
        return button;
    }

    private Control DoneTrailing(BridgeTriageRow row) => row.ImportStatus switch
    {
        BridgeTriageImportStatus.Importing => new Spinner { Width = 14, Height = 14 },
        BridgeTriageImportStatus.Complete
            or BridgeTriageImportStatus.CloudUploadQueued =>
            UploadProgressPresentation.ResolveImport(
                row.ImportStatus,
                _storage.Outbox) switch
            {
                ImportUploadObservation.Awaiting
                    or ImportUploadObservation.Active =>
                    Icons.Glyph(Icons.ArrowUp, 14, "BaeTextSecondaryBrush"),
                ImportUploadObservation.Finished =>
                    DotIcon("BaeSuccessBrush", "✓"),
                _ => throw new InvalidOperationException(
                    "a completed import has no upload observation"),
            },
        BridgeTriageImportStatus.Error => Chip(Loc.Chrome("import.row.failed"), "BaeDangerBrush"),
        // Already imported from a previous session (content-hash match), so
        // there is no in-session status to read — the fact is the same, so the
        // glyph is.
        null => DotIcon("BaeSuccessBrush", "✓"),
        _ => new Panel(),
    };

    private static Control DotIcon(string brushKey, string glyph = "•")
    {
        var text = new TextBlock { Text = glyph, FontSize = 12, FontWeight = FontWeight.Bold };
        text[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(brushKey);
        return text;
    }

    // A tinted mono-text pill: the label at full strength over a wash of the
    // same brush at reduced opacity — two overlapping Borders rather than one,
    // since Avalonia's Opacity applies to the whole subtree (label included),
    // and the label needs to stay fully readable while the fill stays a wash.
    private static Control Chip(string? text, string brushKey)
    {
        var backdrop = new Border { CornerRadius = new CornerRadius(5), Opacity = 0.14 };
        backdrop[!Border.BackgroundProperty] = new DynamicResourceExtension(brushKey);

        var label = new TextBlock { Text = text, FontFamily = new FontFamily("monospace"), FontSize = 10.5 };
        label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(brushKey);
        var foreground = new Border { Padding = new Thickness(7, 3), Child = label };

        var host = new Panel();
        host.Children.Add(backdrop);
        host.Children.Add(foreground);
        return host;
    }

    private ContextMenu? BuildRowContextMenu(BridgeTriageRow row)
    {
        var items = new List<Control>();
        if (row.SkipAction is { } skipAction)
        {
            var shouldSkip = skipAction is BridgeTriageSkipAction.Skip;
            var toggle = new MenuItem
            {
                Header = Loc.Chrome(
                    shouldSkip
                        ? "import.candidate.skip"
                        : "import.candidate.unskip"),
            };
            toggle.Click += (_, _) => _import.SetCandidateSkipped(
                row.CandidateKey,
                shouldSkip);
            items.Add(toggle);
            items.Add(new Separator());
        }
        var reveal = new MenuItem { Header = Loc.Chrome("libraries.reveal") };
        reveal.Click += (_, _) => RevealInFileManager.Reveal(row.CandidateKey);
        items.Add(reveal);
        if (row.CombineAncestorKey is { } combineKey)
        {
            items.Add(new Separator());
            var combine = new MenuItem { Header = Loc.Chrome("import.release.one") };
            combine.Click += (_, _) => ApplyFolderReleaseDecision(
                combineKey,
                BridgeFolderReleaseDecision.CombineAsOneRelease);
            items.Add(combine);
        }
        foreach (var boundary in row.ResolvedBoundaries)
        {
            items.Add(new Separator());
            var decision = boundary.Decision is BridgeFolderReleaseDecision.CombineAsOneRelease
                ? BridgeFolderReleaseDecision.KeepAsSeparateReleases
                : BridgeFolderReleaseDecision.CombineAsOneRelease;
            var regroup = new MenuItem
            {
                Header = Loc.Chrome(
                    decision is BridgeFolderReleaseDecision.CombineAsOneRelease
                        ? "import.release.one"
                        : "import.release.separate"),
            };
            regroup.Click += (_, _) =>
                ApplyFolderReleaseDecision(boundary.Key, decision);
            items.Add(regroup);
        }
        return new ContextMenu { ItemsSource = items };
    }

    private Control BuildInvalidRow(BridgeInvalidCandidate invalid)
    {
        var name = new TextBlock
        {
            Text = invalid.SourceFolderName,
            FontSize = 14,
            FontWeight = FontWeight.SemiBold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        name[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var reasonLine = BridgeDisplay.LocalizedLine(invalid.Reason);
        var reason = new TextBlock
        {
            Text = reasonLine,
            FontSize = 12.5,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Margin = new Thickness(0, 1, 0, 0),
        };
        reason[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeDangerBrush");

        var text = new StackPanel { Spacing = 0, Children = { name, reason } };
        var grid = new Grid { ColumnDefinitions = new ColumnDefinitions("26,44,*"), Margin = new Thickness(9, 7, 10, 7) };

        var warning = new TextBlock { Text = "!", FontWeight = FontWeight.Bold, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
        warning[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeDangerBrush");
        var iconBox = new Border { Width = 44, Height = 44, CornerRadius = new CornerRadius(8), Child = warning };
        iconBox[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        Grid.SetColumn(iconBox, 1);
        Grid.SetColumn(text, 2);
        text.Margin = new Thickness(10, 0, 0, 0);
        text.VerticalAlignment = VerticalAlignment.Center;
        grid.Children.Add(iconBox);
        grid.Children.Add(text);

        var reveal = new MenuItem { Header = Loc.Chrome("libraries.reveal") };
        reveal.Click += (_, _) => RevealInFileManager.Reveal(invalid.FolderPath);

        var host = new Border { Child = grid };
        ToolTip.SetTip(host, reasonLine);
        var items = new List<Control> { reveal };
        foreach (var boundary in invalid.ResolvedBoundaries)
        {
            items.Add(new Separator());
            var decision = boundary.Decision is BridgeFolderReleaseDecision.CombineAsOneRelease
                ? BridgeFolderReleaseDecision.KeepAsSeparateReleases
                : BridgeFolderReleaseDecision.CombineAsOneRelease;
            var regroup = new MenuItem
            {
                Header = Loc.Chrome(
                    decision is BridgeFolderReleaseDecision.CombineAsOneRelease
                        ? "import.release.one"
                        : "import.release.separate"),
            };
            regroup.Click += (_, _) =>
                ApplyFolderReleaseDecision(boundary.Key, decision);
            items.Add(regroup);
        }
        host.ContextMenu = new ContextMenu { ItemsSource = items };
        return host;
    }

    private void ApplyFolderReleaseDecision(
        BridgeFolderReleaseDecisionKey key,
        BridgeFolderReleaseDecision decision)
    {
        _selectedKey = null;
        _pane.Clear();
        _import.SetFolderReleaseDecision(key, decision);
    }

    // ── Row-click activation ────────────────────────────────────────────────

    // Activate a row: kick off auto-identify for a candidate that hasn't been
    // looked at yet (there is nothing to map until it has been looked at),
    // otherwise put it under the mapping pane, read fresh for this key.
    private void OnRowActivated(BridgeTriageRow row)
    {
        if (!row.Actionable)
        {
            return;
        }
        if (row.Placement is BridgeTriagePlacement.NeedsYou
            {
                Reason: BridgeNeedsYouReason.StillIdentifying { Phase: BridgeIdentifyPhase.Queued },
            })
        {
            _ = _import.AutoIdentify(row.CandidateKey);
            return;
        }
        _selectedKey = row.CandidateKey;
        Render();
        _ = _pane.ShowCandidate(row);
    }

    // ── Evidence labels ──────────────────────────────────────────────────────

    // Brand names stay literal — never translated, matching every other
    // provider name in this app's chrome.
    private static string ProviderDisplayName(BridgeMetadataSource source) => source switch
    {
        BridgeMetadataSource.MusicBrainz => "MusicBrainz",
        BridgeMetadataSource.Discogs => "Discogs",
        _ => string.Empty,
    };

    // The row's compact trailing badge. Not a catalog key — an abbreviation of
    // a literal brand name is still the brand's name, not prose.
    private static string ProviderShortCode(BridgeMetadataSource source) => source switch
    {
        BridgeMetadataSource.MusicBrainz => "MB",
        BridgeMetadataSource.Discogs => "DC",
        _ => string.Empty,
    };

    private static string SignalLabel(BridgeMatchedSignal signal) => signal switch
    {
        BridgeMatchedSignal.DiscId => Loc.Chrome("import.row.disc_id"),
        BridgeMatchedSignal.Barcode => Loc.Chrome("import.row.barcode"),
        _ => string.Empty,
    };
}
