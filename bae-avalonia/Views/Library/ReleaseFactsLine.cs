using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The release's facts, and the way into the catalogs that describe it.
///
/// At rest the line reads as a line of facts. When a catalog describes the
/// release, pointing at it fills it softly, and a click toggles a card under it
/// naming them. A release no catalog describes has nothing behind the line, so
/// it is not a trigger at all.
///
/// The card hangs off the line's leading edge, a little below it, with no
/// arrow: it is part of the expansion, not a window pointing back at the line.
/// </summary>
internal sealed class ReleaseFactsLine : ContentControl
{
    /// <summary>How far under the line the card's top sits.</summary>
    private const double CardOffset = 8;

    /// <summary>The card's width, matching the records row it holds.</summary>
    private const double CardWidth = 320;

    private readonly TextBlock _facts = new()
    {
        FontSize = 12,
        FontWeight = FontWeight.Medium,
        MaxLines = 1,
        TextTrimming = TextTrimming.CharacterEllipsis,
        VerticalAlignment = VerticalAlignment.Center,
    };

    internal ReleaseFactsLine()
    {
        _facts[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        HorizontalAlignment = HorizontalAlignment.Left;
    }

    /// <summary>Draw <paramref name="facts"/>, as a trigger when
    /// <paramref name="records"/> names a catalog and as plain text when it
    /// names none.</summary>
    internal void Show(string facts, IReadOnlyList<BridgeReleaseRecord> records)
    {
        _facts.Text = facts;
        IsVisible = facts.Length > 0;
        Content = records.Count == 0 ? _facts : Trigger(records);
    }

    private Control Trigger(IReadOnlyList<BridgeReleaseRecord> records)
    {
        var button = new Button
        {
            Content = _facts,
            Padding = new Thickness(5, 2),
            BorderThickness = new Thickness(0),
            CornerRadius = new CornerRadius(5),
            Background = Brushes.Transparent,
            // The fill bleeds outward from where the line already sat, so the
            // card reads the same at rest as it did before it became a trigger.
            Margin = new Thickness(-5, -2),
            Cursor = new Avalonia.Input.Cursor(
                Avalonia.Input.StandardCursorType.Hand),
        };
        Avalonia.Automation.AutomationProperties.SetAutomationId(button, "release-facts");
        button.PointerEntered += (_, _) =>
        {
            button[!BackgroundProperty] = new DynamicResourceExtension("BaeHoverBrush");
        };
        button.PointerExited += (_, _) =>
        {
            button.Background = Brushes.Transparent;
        };
        var padding = new Thickness(12, 10);
        var card = new Border
        {
            Padding = padding,
            Child = ReleaseRecordsRow.Build(records, ReleaseFactsScale.Card),
            Width = CardWidth,
        };
        var flyout = new Flyout
        {
            Placement = PlacementMode.BottomEdgeAlignedLeft,
            VerticalOffset = CardOffset,
            ShowMode = FlyoutShowMode.Standard,
            Content = card,
        };
        button.Click += (_, _) =>
        {
            if (flyout.IsOpen)
            {
                flyout.Hide();
            }
            else
            {
                flyout.ShowAt(button);
            }
        };
        return button;
    }
}
