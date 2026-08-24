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
    // One item of the list, by kind. Which items exist, in what order, under
    // which header and in which tab is core's answer; this only draws them.
    private Control BuildListCell(BridgeImportListItem item)
    {
        Control control = item switch
        {
            BridgeImportListItem.GroupHeader header => BuildGroupHeader(
                header.Group,
                header.Expanded),
            BridgeImportListItem.Candidate candidate => BuildRow(candidate.Row),
            BridgeImportListItem.Invalid invalid => BuildInvalidRow(invalid.InvalidCandidate),
            _ => new Panel(),
        };
        control.Tag = ImportStore.StableKey(item);
        return control;
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
        button.Click += (_, _) => _import.SetGroupExpanded(group.Key, !expanded);
        if (!group.Combinable)
        {
            return button;
        }

        // The rows below are this folder read as several releases, and this is
        // where it is read as one instead — once, for the folder, rather than
        // on each of the rows it produced. A header that only names a path
        // component the rows share has no such folder behind it.
        var combine = BuildActionPill(
            Loc.Chrome("import.release.one"),
            () => ApplyFolderReleaseDecision(
                group.Key,
                BridgeFolderReleaseDecision.CombineAsOneRelease));
        combine.HorizontalAlignment = HorizontalAlignment.Right;
        combine.VerticalAlignment = VerticalAlignment.Center;
        combine.Margin = new Thickness(0, 0, 10, 0);
        var host = new Panel();
        host.Children.Add(button);
        host.Children.Add(combine);
        return host;
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

        // A running import is the one line on a row that changes by the
        // second, so it draws itself off the candidate-runtime signal rather
        // than waiting for the queue to re-project. The row says only *that*
        // an import is running, which is what the queue does answer.
        if (row.ImportStatus is BridgeTriageImportStatus.Importing)
        {
            column.Children.Add(ImportProgressLine.Build(_import, row.CandidateKey));
        }
        else if (row.Placement is BridgeTriagePlacement.Ready)
        {
            // A Ready row says the same three things the pane header does —
            // the title above, then who it is by, then the pressing — rather
            // than packing all of it into one line.
            if (row.Matched?.Artist is { Length: > 0 } artist)
            {
                column.Children.Add(ReadyLine(artist, 12.5, opacity: 1));
            }
            if (PressingLine(row) is { Length: > 0 } pressing)
            {
                column.Children.Add(ReadyLine(pressing, 11.5, opacity: 0.7));
            }
        }
        else
        {
            if (RowSubLine(row) is { Length: > 0 } subLine)
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

            if (upload is ImportUploadObservation.Active)
            {
                var bar = CloudProgressBar(upload);
                bar.Margin = new Thickness(0, 7, 0, 0);
                column.Children.Add(bar);
            }
        }

        var actions = BuildRowActions(row);
        if (actions is not null)
        {
            actions.Margin = new Thickness(0, 7, 0, 0);
            column.Children.Add(actions);
        }

        return column;
    }

    // One of the Ready row's own lines. This app has no third text brush, so
    // the pressing line reads as one step quieter through opacity.
    private static Control ReadyLine(string text, double fontSize, double opacity)
    {
        var line = new TextBlock
        {
            Text = text,
            FontSize = fontSize,
            Opacity = opacity,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Margin = new Thickness(0, 1, 0, 0),
        };
        line[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return line;
    }

    // The second line: the matched release's metadata, a disagreement sentence,
    // the still-identifying phase, or an import failure — whichever `row` is
    // actually saying. A Ready row draws its own lines and is not asked.
    private string? RowSubLine(BridgeTriageRow row) => row.Placement switch
    {
        BridgeTriagePlacement.Skipped => MetadataLine(row),
        BridgeTriagePlacement.NeedsYou { Reason: BridgeNeedsYouReason.StillIdentifying phase } =>
            BridgeDisplay.LocalizedLine(phase.Phase),
        BridgeTriagePlacement.NeedsYou(BridgeNeedsYouGroup.AlreadyInLibrary, BridgeNeedsYouReason.Disagreement) =>
            MetadataLine(row),
        BridgeTriagePlacement.NeedsYou(BridgeNeedsYouGroup.PickAPressing, BridgeNeedsYouReason.Disagreement) =>
            row.Matched?.Artist,
        BridgeTriagePlacement.NeedsYou { Reason: BridgeNeedsYouReason.Disagreement disagreement } =>
            BridgeDisplay.LocalizedLine(disagreement.DisagreementValue),
        BridgeTriagePlacement.Importing or BridgeTriagePlacement.Failed or BridgeTriagePlacement.Done =>
            ImportSubLine(row),
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

    // The pressing on its own: `CD · 1991 · 10 tracks`, with whatever the
    // source did not say left out.
    private static string? PressingLine(BridgeTriageRow row)
    {
        if (row.Matched?.Pressing is not { } pressing)
        {
            return null;
        }
        var parts = new List<string>();
        if (!string.IsNullOrEmpty(pressing.Format))
        {
            parts.Add(pressing.Format!);
        }
        if (pressing.Year is { } year)
        {
            parts.Add(year.ToString(CultureInfo.CurrentCulture));
        }
        if (pressing.TrackCount is { } trackCount)
        {
            parts.Add(Loc.Chrome("import.candidate.tracks", "count", (long)trackCount));
        }
        return parts.Count == 0 ? null : string.Join(" · ", parts);
    }

    private static string? ImportSubLine(BridgeTriageRow row) => row.ImportStatus switch
    {
        // A running import draws its own line; this is not asked for it.
        BridgeTriageImportStatus.Importing => null,
        // The row says what the release is. That its files are still going up
        // is the trailing glyph's to say, and how far they have come is the
        // bar's — a count of the files queued behind it answers a question
        // nobody asked of a row.
        BridgeTriageImportStatus.Complete or null => MetadataLine(row),
        BridgeTriageImportStatus.Error error => BridgeDisplay.LocalizedLine(error.ErrorValue),
        _ => null,
    };

    private static ProgressBar CloudProgressBar(
        ImportUploadObservation observation)
    {
        var fraction = observation switch
        {
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

    // The pill row under the meta column — only the placement the design gives
    // one: already-in-library. Every other row's action is "activate it," which
    // the whole row already does. How the folder around the row is read is not
    // one of them: that lives on the group header, or in the row's own menu
    // where the folder is this one row.
    private Control? BuildRowActions(BridgeTriageRow row)
    {
        if (row.Placement is not BridgeTriagePlacement.NeedsYou(
                BridgeNeedsYouGroup.AlreadyInLibrary, BridgeNeedsYouReason.Disagreement))
        {
            return null;
        }
        var pills = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        pills.Children.Add(BuildActionPill(
            Loc.Chrome("import.row.import_anyway"), () => OnRowActivated(row)));
        return pills;
    }

    private static Button BuildActionPill(string label, Action onClick)
    {
        var button = new Button
        {
            Content = label,
            FontSize = 12,
            FontWeight = FontWeight.Medium,
            Padding = new Thickness(12, 4),
            CornerRadius = new CornerRadius(999),
            BorderThickness = new Thickness(1),
            Background = Brushes.Transparent,
        };
        button[!Button.BorderBrushProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        button[!Button.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        button.Click += (_, _) => onClick();
        button.Tapped += (_, eventArgs) => eventArgs.Handled = true;
        return button;
    }

    private Control BuildRowTrailing(BridgeTriageRow row)
    {
        switch (row.Placement)
        {
            case BridgeTriagePlacement.Ready:
                return Chip(Loc.Chrome("import.row.ready"), "BaeSuccessBrush");
            case BridgeTriagePlacement.NeedsYou(var group, var reason):
                return NeedsYouTrailing(row, group, reason);
            case BridgeTriagePlacement.Importing:
                return new Spinner { Width = 14, Height = 14 };
            case BridgeTriagePlacement.Failed:
            case BridgeTriagePlacement.Done:
                return ImportTrailing(row);
            default:
                return new Panel();
        }
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
        var button = BuildActionPill(Loc.Chrome("import.row.search_manually"), () => OnRowActivated(row));
        button.Padding = new Thickness(7, 3);
        return button;
    }

    // What a row past the point of being asked anything shows: the running
    // import's spinner, the failure's tag, or the completed import's mark and
    // its cloud transition.
    private Control ImportTrailing(BridgeTriageRow row) => row.ImportStatus switch
    {
        BridgeTriageImportStatus.Importing => new Spinner { Width = 14, Height = 14 },
        BridgeTriageImportStatus.Complete =>
            UploadProgressPresentation.ResolveImport(
                row.ImportStatus,
                _storage.Outbox) switch
            {
                ImportUploadObservation.Active =>
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
        // A folder read as one release is this row and nothing else, so its row
        // is the only place left to say otherwise. A folder read as several is
        // a group of rows, and its header carries that choice.
        foreach (var boundary in Combined(row.ResolvedBoundaries))
        {
            items.Add(new Separator());
            var regroup = new MenuItem { Header = Loc.Chrome("import.release.separate") };
            regroup.Click += (_, _) => ApplyFolderReleaseDecision(
                boundary.Key,
                BridgeFolderReleaseDecision.KeepAsSeparateReleases);
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
        // A folder read as one release that turned out to be unreadable is
        // still that folder, and its row is the only place left to say it
        // should be read as several.
        foreach (var boundary in Combined(invalid.ResolvedBoundaries))
        {
            items.Add(new Separator());
            var regroup = new MenuItem { Header = Loc.Chrome("import.release.separate") };
            regroup.Click += (_, _) => ApplyFolderReleaseDecision(
                boundary.Key,
                BridgeFolderReleaseDecision.KeepAsSeparateReleases);
            items.Add(regroup);
        }
        host.ContextMenu = new ContextMenu { ItemsSource = items };
        return host;
    }

    /// <summary>The settled readings that say "this folder is one release" —
    /// the only ones a row can offer to reverse.</summary>
    private static IEnumerable<BridgeResolvedFolderReleaseBoundary> Combined(
        IEnumerable<BridgeResolvedFolderReleaseBoundary> boundaries) =>
        boundaries.Where(boundary =>
            boundary.Decision is BridgeFolderReleaseDecision.CombineAsOneRelease);

    private void ApplyFolderReleaseDecision(
        BridgeFolderReleaseDecisionKey key,
        BridgeFolderReleaseDecision decision)
    {
        _selectedKey = null;
        _import.ClearObservedCandidate();
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
        SelectCandidate(row.CandidateKey);
    }

    // Put the candidate at `key` under the pane. Its folder, its files and its
    // resumed identify state come from its own query, so a key whose row is not
    // in a loaded window opens the pane as soon as that query answers.
    private void SelectCandidate(string key)
    {
        _selectedKey = key;
        _import.ObserveCandidate(key);
        Render();
        if (_import.Row(key) is { } row)
        {
            _pendingSelection = null;
            _ = _pane.ShowCandidate(row);
        }
        else
        {
            _pendingSelection = key;
        }
    }

    // The row a pending selection was waiting for has arrived.
    private void ShowPendingSelection()
    {
        if (_pendingSelection is not { } key || _import.Row(key) is not { } row)
        {
            return;
        }
        _pendingSelection = null;
        _ = _pane.ShowCandidate(row);
    }

}
