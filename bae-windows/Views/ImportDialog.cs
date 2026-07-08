using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The import dialog: a folder scan source plus the live candidate list bound to
// the import store. Clicking an unidentified candidate kicks off auto-identify;
// once it has a result, clicking opens the picker. Shared by the toolbar import
// button and the folder-drop handler.
internal sealed class ImportDialog
{
    private readonly SessionStore _session;
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Func<IntPtr> _windowHandle;
    private readonly ImportStore _import;
    private readonly ImportPickerDialog _picker;

    // True while the dialog is showing. The window-drop handler reads it to avoid
    // opening a second ContentDialog over an already-open import dialog.
    public bool IsOpen { get; private set; }

    public ImportDialog(
        SessionStore session,
        Func<XamlRoot?> xamlRoot,
        Func<IntPtr> windowHandle,
        ImportStore import,
        ImportPickerDialog picker)
    {
        _session = session;
        _xamlRoot = xamlRoot;
        _windowHandle = windowHandle;
        _import = import;
        _picker = picker;
    }

    public async System.Threading.Tasks.Task Show()
    {
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }
        _import.RefreshCandidates();

        var scanButton = new Button { Content = Loc.Chrome("import.choose_folder") };
        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        // The candidate refresh reports its no-releases line through the store;
        // render it into the status block without changing visibility (a scan
        // makes the block visible), matching the prior in-place update.
        void RenderScanStatus() => status.Text = _import.CandidatesStatusText;
        _import.CandidatesRefreshed += RenderScanStatus;

        var list = new ListView
        {
            ItemsSource = _import.Candidates,
            SelectionMode = ListViewSelectionMode.None,
            IsItemClickEnabled = true,
            MaxHeight = 320,
        };

        // Each row builds imperatively: the candidate's one-line summary plus a
        // signals badge row. ContainerContentChanging fires per recycled container
        // (re-firing when UpdateCandidate swaps in a fresh instance), so the badges
        // refresh as identify events land.
        list.ContainerContentChanging += (_, args) =>
        {
            if (args.InRecycleQueue || args.Item is not ImportCandidate candidate)
            {
                return;
            }

            args.ItemContainer.Content = BuildCandidateRow(candidate);
            args.Handled = true;
        };

        // First click on an unidentified candidate kicks off auto-identification;
        // once it has a result, clicking opens the import dialog (auto matches
        // plus a manual-search fallback). The row reflects status from the
        // candidate snapshot.
        list.ItemClick += async (_, args) =>
        {
            if (args.ClickedItem is not ImportCandidate candidate)
            {
                return;
            }

            if (string.IsNullOrEmpty(candidate.Status))
            {
                _ = _import.AutoIdentify(candidate.Key, candidate.FolderPath);
            }
            else
            {
                await _picker.Show(candidate);
            }
        };

        // The picker needs the app window's handle in an unpackaged app. Adding
        // a watched folder starts scanning; invalidations refresh the candidates.
        scanButton.Click += async (_, _) =>
        {
            var picker = new global::Windows.Storage.Pickers.FolderPicker();
            picker.FileTypeFilter.Add("*");
            WinRT.Interop.InitializeWithWindow.Initialize(picker, _windowHandle());
            var folder = await picker.PickSingleFolderAsync();
            if (folder is null)
            {
                return;
            }

            status.Text = Loc.Chrome("import.scanning");
            status.Visibility = Visibility.Visible;
            var path = folder.Path;
            var (current, error) = await _import.ScanFolder(path);
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                status.Text = error;
            }
            else
            {
                _import.RefreshCandidates();
            }
        };

        var content = new StackPanel { Spacing = 8, Width = 420 };
        content.Children.Add(scanButton);
        content.Children.Add(status);
        content.Children.Add(list);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("import.title"),
            Content = content,
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = _xamlRoot(),
        };

        IsOpen = true;
        try
        {
            await dialog.ShowAsync();
        }
        finally
        {
            IsOpen = false;
            _import.CandidatesRefreshed -= RenderScanStatus;
        }
    }

    // One candidate row: the summary line, then a signals badge row underneath
    // (omitted until the first toolbar event lands). Core pre-shapes every badge;
    // this iterates and renders.
    private StackPanel BuildCandidateRow(ImportCandidate candidate)
    {
        var row = new StackPanel { Spacing = 4 };
        row.Children.Add(new TextBlock
        {
            Text = candidate.ToString(),
            TextWrapping = TextWrapping.Wrap,
        });

        if (candidate.Signals.Count > 0)
        {
            var badges = new StackPanel
            {
                Orientation = Orientation.Horizontal,
                VerticalAlignment = VerticalAlignment.Center,
            };
            foreach (var signal in candidate.Signals)
            {
                badges.Children.Add(BuildSignalBadge(candidate.Key, signal));
            }

            badges.Children.Add(BuildRerunButton(candidate.Key));
            row.Children.Add(badges);
        }

        return row;
    }

    // The trailing re-run control on the signals row: re-dispatches the
    // candidate's lookups (keeping the user's exclusions). The re-derived state
    // arrives through candidate invalidation.
    private Button BuildRerunButton(string candidateKey)
    {
        var button = new Button
        {
            Content = "↻",
            Padding = new Thickness(8, 3, 8, 3),
            VerticalAlignment = VerticalAlignment.Center,
        };
        ToolTipService.SetToolTip(button, Loc.Chrome("import.rerun_identify"));
        button.Click += (_, _) =>
        {
            if (_session.CurrentHandleOrNull() != null)
            {
                _ = _import.RerunIdentify(candidateKey);
            }
        };
        return button;
    }

    // One signals badge: a kind label, the value (truncated), and a trailing
    // state visual (spinner / count / dash / warning). Excluded badges dim and
    // strike through but stay in place so the row's layout is stable. Clicking a
    // badge toggles its signal in/out of triangulation (excluded badges re-include
    // on click); the re-derived toolbar arrives through candidate invalidation.
    // Mirrors the macOS SignalBadge anatomy in plain WinUI primitives.
    private Button BuildSignalBadge(string candidateKey, SignalBadge signal)
    {
        var inner = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
        };

        var label = new TextBlock
        {
            Text = SignalKindLabel(signal.Kind),
            FontSize = 12,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            VerticalAlignment = VerticalAlignment.Center,
        };
        if (signal.Excluded)
        {
            label.TextDecorations = global::Windows.UI.Text.TextDecorations.Strikethrough;
        }

        inner.Children.Add(label);

        if (!string.IsNullOrEmpty(signal.Value))
        {
            var value = new TextBlock
            {
                Text = TextTruncation.MiddleTruncate(signal.Value, 20),
                FontSize = 11,
                FontFamily = new FontFamily("Consolas"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                MaxWidth = 140,
                // The character budget keeps the value under MaxWidth; CharacterEllipsis
                // is only a backstop if a font measures wider than monospace estimates.
                TextTrimming = TextTrimming.CharacterEllipsis,
                VerticalAlignment = VerticalAlignment.Center,
            };
            if (signal.Excluded)
            {
                value.TextDecorations = global::Windows.UI.Text.TextDecorations.Strikethrough;
            }

            inner.Children.Add(value);
        }

        inner.Children.Add(BuildSignalState(signal));

        // A Button, not a Border+Tapped: the ListView raises ItemClick from its own
        // gesture handling and ignores a child that merely marks Tapped handled, but
        // it honors a pointer-capturing control. So the badge press toggles the
        // signal without also re-triggering auto-identify / opening the import
        // dialog. MinWidth/Height 0 keeps it badge-sized, not the default button box.
        var badge = new Button
        {
            Content = inner,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Padding = new Thickness(8, 3, 8, 3),
            MinWidth = 0,
            MinHeight = 0,
            CornerRadius = new CornerRadius(8),
            BorderThickness = new Thickness(1),
            BorderBrush = new SolidColorBrush(Microsoft.UI.Colors.DimGray),
            Margin = new Thickness(0, 0, 6, 0),
            Opacity = signal.Excluded ? 0.45 : 1.0,
        };

        // Clicking toggles this signal's exclusion. Excluded badges stay clickable
        // (to re-include). The catalog kind names a specific candidate by its value;
        // disc_id / barcode are singletons (value ignored core-side).
        ToolTipService.SetToolTip(
            badge,
            signal.Excluded ? Loc.Chrome("signal.include") : Loc.Chrome("signal.exclude"));
        badge.Click += (_, _) =>
        {
            if (_session.CurrentHandleOrNull() != null)
            {
                var kind = signal.Kind;
                var value = signal.Value ?? string.Empty;
                _ = _import.ToggleSignal(candidateKey, kind, value);
            }
        };
        return badge;
    }

    // The badge's trailing state visual, chosen by the pre-shaped SignalState the
    // generated bridge carried over. An excluded badge shows the exclusion mark regardless.
    private static FrameworkElement BuildSignalState(SignalBadge signal)
    {
        if (signal.Excluded)
        {
            return new TextBlock
            {
                Text = "✕",
                FontSize = 11,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                VerticalAlignment = VerticalAlignment.Center,
            };
        }

        switch (signal.State.Kind)
        {
            case "looking_up":
                return new ProgressRing { IsActive = true, Width = 14, Height = 14 };
            case "found":
                return CountPill((signal.State.Count ?? 0).ToString(), Microsoft.UI.Colors.LightGreen);
            case "confirms":
                return signal.State.Count is > 0
                    ? new TextBlock
                    {
                        Text = "✓",
                        FontSize = 12,
                        FontWeight = Microsoft.UI.Text.FontWeights.Bold,
                        Foreground = new SolidColorBrush(Microsoft.UI.Colors.DeepSkyBlue),
                        VerticalAlignment = VerticalAlignment.Center,
                    }
                    : CountPill("0", Microsoft.UI.Colors.Gray);
            case "no_match":
                return CountPill("0", Microsoft.UI.Colors.Gray);
            case "skipped":
                return new TextBlock
                {
                    Text = "–",
                    FontSize = 12,
                    Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                    VerticalAlignment = VerticalAlignment.Center,
                };
            case "failed":
                var warning = new TextBlock
                {
                    Text = "⚠",
                    FontSize = 12,
                    Foreground = new SolidColorBrush(Microsoft.UI.Colors.Orange),
                    VerticalAlignment = VerticalAlignment.Center,
                };
                // The structured lookup failure resolves its localized line for
                // the hover tooltip; no prose crosses the bridge.
                if (signal.State.Failure is { } failure)
                {
                    ToolTipService.SetToolTip(warning, BridgeDisplay.LocalizedLine(failure));
                }
                return warning;
            default:
                return new TextBlock { Text = string.Empty };
        }
    }

    // The badge's kind label. Mirrors the macOS SignalBadgeStyle.label(for:);
    // the wire kind names come from the generated bridge's snake_case mapping, resolved to a
    // localized chrome label.
    private static string SignalKindLabel(string kind) => kind switch
    {
        "disc_id" => Loc.Chrome("signal.kind.disc_id"),
        "barcode" => Loc.Chrome("signal.kind.barcode"),
        "catalog" => Loc.Chrome("signal.kind.catalog"),
        _ => kind,
    };

    // A small count pill — a colored digit, the badge's settled-state readout.
    private static Border CountPill(string text, global::Windows.UI.Color color)
    {
        return new Border
        {
            Child = new TextBlock
            {
                Text = text,
                FontSize = 11,
                FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
                Foreground = new SolidColorBrush(color),
            },
            Padding = new Thickness(6, 1, 6, 1),
            CornerRadius = new CornerRadius(6),
            VerticalAlignment = VerticalAlignment.Center,
        };
    }
}
