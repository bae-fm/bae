using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;

namespace Bae.Desktop;

// The signals-toolbar badge row used by the release re-identify dialog: one
// badge per signal (kind label, truncated value, lookup-state visual) and a
// trailing re-run control. Clicking a badge toggles its signal in or out of
// triangulation; the re-derived state arrives through candidate invalidation.
// Every color reads a theme brush.
internal static class SignalBadgeRow
{
    public static Control Build(
        IReadOnlyList<SignalBadge> signals,
        Action<string, string> onToggleSignal,
        Action onRerun)
    {
        var badges = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        foreach (var signal in signals)
        {
            badges.Children.Add(BuildBadge(signal, onToggleSignal));
        }
        badges.Children.Add(RerunButton(onRerun));
        return badges;
    }

    private static Button RerunButton(Action onRerun)
    {
        var button = new Button
        {
            Content = "↻",
            Padding = new Thickness(8, 3),
            VerticalAlignment = VerticalAlignment.Center,
        };
        ToolTip.SetTip(button, Loc.Chrome("import.rerun_identify"));
        button.Click += (_, _) => onRerun();
        return button;
    }

    private static Button BuildBadge(SignalBadge signal, Action<string, string> onToggleSignal)
    {
        var inner = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6, VerticalAlignment = VerticalAlignment.Center };

        var label = new TextBlock
        {
            Text = SignalKindLabel(signal.Kind),
            FontSize = 12,
            FontWeight = FontWeight.SemiBold,
            VerticalAlignment = VerticalAlignment.Center,
            TextDecorations = signal.Excluded ? TextDecorations.Strikethrough : null,
        };
        label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        inner.Children.Add(label);

        if (!string.IsNullOrEmpty(signal.Value))
        {
            var value = new TextBlock
            {
                Text = TextTruncation.MiddleTruncate(signal.Value!, 20),
                FontSize = 11,
                FontFamily = new FontFamily("monospace"),
                MaxWidth = 140,
                TextTrimming = TextTrimming.CharacterEllipsis,
                VerticalAlignment = VerticalAlignment.Center,
                TextDecorations = signal.Excluded ? TextDecorations.Strikethrough : null,
            };
            value[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            inner.Children.Add(value);
        }

        inner.Children.Add(BuildState(signal));

        var badge = new Button
        {
            Content = inner,
            Background = Brushes.Transparent,
            Padding = new Thickness(8, 3),
            MinWidth = 0,
            MinHeight = 0,
            CornerRadius = new CornerRadius(8),
            BorderThickness = new Thickness(1),
            Margin = new Thickness(0, 0, 6, 0),
            Opacity = signal.Excluded ? 0.45 : 1.0,
        };
        badge[!Button.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        ToolTip.SetTip(badge, signal.Excluded ? Loc.Chrome("signal.include") : Loc.Chrome("signal.exclude"));
        badge.Click += (_, _) => onToggleSignal(signal.Kind, signal.Value ?? string.Empty);
        return badge;
    }

    private static Control BuildState(SignalBadge signal)
    {
        if (signal.Excluded)
        {
            return StateGlyph("✕", "BaeTextSecondaryBrush");
        }
        switch (signal.State.Kind)
        {
            case "looking_up":
                return new Spinner { Width = 14, Height = 14, VerticalAlignment = VerticalAlignment.Center };
            case "found":
                return CountPill((signal.State.Count ?? 0).ToString(), "BaeSuccessBrush");
            case "confirms":
                return signal.State.Count is > 0
                    ? StateGlyph("✓", "BaeAccentBrush", bold: true)
                    : CountPill("0", "BaeTextSecondaryBrush");
            case "no_match":
                return CountPill("0", "BaeTextSecondaryBrush");
            case "skipped":
                return StateGlyph("–", "BaeTextSecondaryBrush");
            case "failed":
                var warning = StateGlyph("⚠", "BaeDangerBrush");
                if (signal.State.Failure is { } failure)
                {
                    ToolTip.SetTip(warning, BridgeDisplay.LocalizedLine(failure));
                }
                return warning;
            default:
                return new TextBlock();
        }
    }

    private static TextBlock StateGlyph(string text, string brushKey, bool bold = false)
    {
        var glyph = new TextBlock
        {
            Text = text,
            FontSize = 12,
            FontWeight = bold ? FontWeight.Bold : FontWeight.Normal,
            VerticalAlignment = VerticalAlignment.Center,
        };
        glyph[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(brushKey);
        return glyph;
    }

    private static Border CountPill(string text, string brushKey)
    {
        var digit = new TextBlock { Text = text, FontSize = 11, FontWeight = FontWeight.SemiBold };
        digit[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(brushKey);
        return new Border
        {
            Child = digit,
            Padding = new Thickness(6, 1),
            CornerRadius = new CornerRadius(6),
            VerticalAlignment = VerticalAlignment.Center,
        };
    }

    private static string SignalKindLabel(string kind) => kind switch
    {
        "disc_id" => Loc.Chrome("signal.kind.disc_id"),
        "barcode" => Loc.Chrome("signal.kind.barcode"),
        "catalog" => Loc.Chrome("signal.kind.catalog"),
        _ => kind,
    };
}
