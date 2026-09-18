using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The release's facts, and the way into where they came from.
///
/// At rest the line reads as a line of facts. When the release carries names of
/// its own, the rip databases confirmed its audio, or a catalog describes it,
/// pointing at it fills it softly and fades a seal in at its tail, and a click
/// toggles a card under it stating them. A release with none of the three has
/// nothing behind the line, so it is not a trigger at all.
///
/// The card hangs off the line's leading edge, a little below it, with no
/// arrow: it is part of the expansion, not a window pointing back at the line.
/// </summary>
internal sealed class ReleaseFactsLine : ContentControl
{
    /// <summary>How far under the line the card's top sits.</summary>
    private const double CardOffset = 8;

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

    /// <summary>Draw <paramref name="facts"/>, as a trigger when the release
    /// states a name of its own, <paramref name="verification"/> says the rip
    /// databases confirmed it, or <paramref name="records"/> names a catalog,
    /// and as plain text when it has none of them.</summary>
    internal void Show(
        string facts,
        IReadOnlyList<BridgeReleaseMark> marks,
        BridgeVerification? verification,
        IReadOnlyList<BridgeReleaseRecord> records)
    {
        _facts.Text = facts;
        IsVisible = facts.Length > 0;
        Content = marks.Count == 0 && records.Count == 0
            && verification?.MatchedCopies is null
            ? _facts
            : Trigger(marks, verification, records);
    }

    private Control Trigger(
        IReadOnlyList<BridgeReleaseMark> marks,
        BridgeVerification? verification,
        IReadOnlyList<BridgeReleaseRecord> records)
    {
        var seal = Icons.Glyph(Icons.Seal, 11, "BaeTextSecondaryBrush");
        seal.Opacity = 0;
        seal.VerticalAlignment = VerticalAlignment.Center;
        Avalonia.Automation.AutomationProperties.SetName(
            seal, Loc.Core("core.identity.identified"));
        // The seal fades rather than snaps: it is a hint at the line's tail,
        // not a control that appears.
        seal.Transitions =
        [
            new DoubleTransition
            {
                Property = OpacityProperty,
                Duration = TimeSpan.FromSeconds(0.15),
            },
        ];
        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            Children = { _facts, seal },
        };
        var button = new Button
        {
            Content = row,
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
            seal.Opacity = 1;
            button[!BackgroundProperty] = new DynamicResourceExtension("BaeHoverBrush");
        };
        button.PointerExited += (_, _) =>
        {
            seal.Opacity = 0;
            button.Background = Brushes.Transparent;
        };
        var flyout = new Flyout
        {
            Placement = PlacementMode.BottomEdgeAlignedLeft,
            VerticalOffset = CardOffset,
            ShowMode = FlyoutShowMode.Standard,
            Content = ReleaseFactsFlyout.Build(marks, verification, records),
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
