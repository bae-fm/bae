using System;
using System.Collections.Generic;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The signals-toolbar badge row shared by the import candidate list and the
// re-identify dialog: one badge per signal (kind label, truncated value,
// lookup-state visual), a trailing re-run control. Clicking a badge toggles
// its signal in/out of triangulation; the re-derived state arrives through
// candidate invalidation.
internal static class SignalBadgeRow
{
    internal static StackPanel Build(
        IReadOnlyList<SignalBadge> signals,
        Action<string, string> onToggleSignal,   // (kind, value)
        Action onRerun)
    {
        var badges = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            VerticalAlignment = VerticalAlignment.Center,
        };
        foreach (var signal in signals)
        {
            badges.Children.Add(BuildSignalBadge(signal, onToggleSignal));
        }

        badges.Children.Add(BuildRerunButton(onRerun));
        return badges;
    }

    // The trailing re-run control on the signals row: re-dispatches the
    // candidate's lookups (keeping the user's exclusions). The re-derived state
    // arrives through candidate invalidation.
    private static Button BuildRerunButton(Action onRerun)
    {
        var button = new Button
        {
            Content = "↻",
            Padding = new Thickness(8, 3, 8, 3),
            VerticalAlignment = VerticalAlignment.Center,
        };
        ToolTipService.SetToolTip(button, Loc.Chrome("import.rerun_identify"));
        button.Click += (_, _) => onRerun();
        return button;
    }

    // One signals badge: a kind label, the value (truncated), and a trailing
    // state visual (spinner / count / dash / warning). Excluded badges dim and
    // strike through but stay in place so the row's layout is stable. Clicking a
    // badge toggles its signal in/out of triangulation (excluded badges re-include
    // on click); the re-derived toolbar arrives through candidate invalidation.
    // Mirrors the macOS SignalBadge anatomy in plain WinUI primitives.
    private static Button BuildSignalBadge(SignalBadge signal, Action<string, string> onToggleSignal)
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
            var kind = signal.Kind;
            var value = signal.Value ?? string.Empty;
            onToggleSignal(kind, value);
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
