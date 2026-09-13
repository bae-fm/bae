using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;

namespace Bae.Desktop;

// The signals-toolbar badge row: one badge per signal (kind label, truncated
// value, lookup-state visual), then the catalog numbers the answers carry as
// chips, then a trailing re-run control. A signal that offers several values —
// the catalog over every number extracted from the candidate, the barcode over
// every code read off it — opens its list, each value checked when the run asks
// about it; a signal with one value is the switch for that value. Clicking a
// chip strikes its number out of what the folder is taken to state, or counts
// it again; nothing is looked up either way. The re-derived state arrives
// through the candidate stream. Every color reads a theme brush.
internal static class SignalBadgeRow
{
    public static Control Build(
        IReadOnlyList<SignalBadge> signals,
        IReadOnlyList<CatalogAgreement> agreements,
        Action<string, string> onToggleSignal,
        Action<string> onToggleAgreement,
        Action onRerun)
    {
        var badges = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        foreach (var signal in signals)
        {
            badges.Children.Add(Badge(signal, onToggleSignal));
        }
        foreach (var agreement in agreements)
        {
            badges.Children.Add(BuildAgreementChip(agreement, onToggleAgreement));
        }
        badges.Children.Add(RerunButton(onRerun));
        return badges;
    }

    // One catalog number the folder states about a release the run is
    // offering. Counted, it carries the accent the matched rows carry; struck
    // out, it is dimmed and struck through, and stands as the way back.
    private static Button BuildAgreementChip(
        CatalogAgreement agreement, Action<string> onToggleAgreement)
    {
        var counted = !agreement.Discounted;
        var inner = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 5,
            VerticalAlignment = VerticalAlignment.Center,
        };
        var check = new TextBlock
        {
            Text = counted ? "✓" : "○",
            FontSize = 11,
            FontWeight = FontWeight.SemiBold,
            VerticalAlignment = VerticalAlignment.Center,
        };
        check[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(
            counted ? "BaeAccentBrush" : "BaeTextSecondaryBrush");
        inner.Children.Add(check);

        var number = new TextBlock
        {
            Text = TextTruncation.MiddleTruncate(agreement.Value, 20),
            FontSize = 11,
            FontFamily = new FontFamily("monospace"),
            MaxWidth = 140,
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
            TextDecorations = counted ? null : TextDecorations.Strikethrough,
        };
        number[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(
            counted ? "BaeAccentBrush" : "BaeTextSecondaryBrush");
        inner.Children.Add(number);

        var chip = new Button
        {
            Content = inner,
            Background = Brushes.Transparent,
            Padding = new Thickness(8, 3),
            MinWidth = 0,
            MinHeight = 0,
            CornerRadius = new CornerRadius(8),
            BorderThickness = new Thickness(1),
            Margin = new Thickness(0, 0, 6, 0),
            Opacity = counted ? 1.0 : 0.45,
        };
        chip[!Button.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        ToolTip.SetTip(chip, Loc.Chrome(counted
            ? "signal.catalog_stop_counting"
            : "signal.catalog_count_again"));
        chip.Click += (_, _) => onToggleAgreement(agreement.Value);
        return chip;
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

    // What one signal's badge is: the list of its values where it offers
    // several, the switch for its one value where it names one, and neither
    // where the candidate turned up nothing for it to act on.
    private static Control Badge(SignalBadge signal, Action<string, string> onToggleSignal)
    {
        if (signal.Options.Count > 0)
        {
            return BuildChoiceBadge(signal, onToggleSignal);
        }
        return BuildBadge(
            signal,
            string.IsNullOrEmpty(signal.Value) ? null : onToggleSignal);
    }

    // A signal that offers several values: the chip opens the list, each entry
    // checked when the run asks about that value. Checking one leaves the rest
    // as they are — several codes and several numbers can be asked about at
    // once.
    private static Control BuildChoiceBadge(SignalBadge signal, Action<string, string> onToggleSignal)
    {
        var badge = BuildBadge(signal, null);
        var items = new List<Control>();
        foreach (var option in signal.Options)
        {
            var value = option.Value;
            var item = new MenuItem
            {
                Header = (option.Chosen ? "✓ " : string.Empty) + value,
            };
            item.Click += (_, _) => onToggleSignal(signal.Kind, value);
            items.Add(item);
        }
        badge.Flyout = new MenuFlyout { ItemsSource = items };
        ToolTip.SetTip(badge, Loc.Chrome(PickTipKey(signal.Kind)));
        return badge;
    }

    private static string PickTipKey(string kind) => kind switch
    {
        "barcode" => "signal.pick_barcode",
        _ => "signal.pick_catalog",
    };

    // `onToggleSignal` is null for a badge with nothing to switch: one that
    // opens a list instead, and one the candidate turned up no value for.
    private static Button BuildBadge(SignalBadge signal, Action<string, string>? onToggleSignal)
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
        if (onToggleSignal is { } toggle)
        {
            ToolTip.SetTip(badge, signal.Excluded ? Loc.Chrome("signal.include") : Loc.Chrome("signal.exclude"));
            badge.Click += (_, _) => toggle(signal.Kind, signal.Value ?? string.Empty);
        }
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
