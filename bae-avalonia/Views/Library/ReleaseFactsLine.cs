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
/// The release's facts, and the way into where they came from.
///
/// At rest the line reads as a line of facts. When the release carries names of
/// its own, the rip databases confirmed its audio, or a catalog describes it,
/// pointing at it fills it softly and shows a seal at its tail, and a click
/// opens a flyout stating them. A release with none of the three has nothing
/// behind the line, so it is not a trigger at all.
/// </summary>
internal sealed class ReleaseFactsLine : ContentControl
{
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
        Content = marks.Count == 0 && records.Count == 0 && verification is null
            ? _facts
            : Trigger(marks, verification, records);
    }

    private Control Trigger(
        IReadOnlyList<BridgeReleaseMark> marks,
        BridgeVerification? verification,
        IReadOnlyList<BridgeReleaseRecord> records)
    {
        var seal = Icons.Glyph(Icons.Seal, 12, "BaeTextSecondaryBrush");
        seal.Opacity = 0;
        seal.VerticalAlignment = VerticalAlignment.Center;
        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            Children = { _facts, seal },
        };
        var button = new Button
        {
            Content = row,
            Padding = new Thickness(6, 3),
            BorderThickness = new Thickness(0),
            CornerRadius = new CornerRadius(6),
            Background = Brushes.Transparent,
            // The fill bleeds outward from where the line already sat, so the
            // card reads the same at rest as it did before it became a trigger.
            Margin = new Thickness(-6, -3),
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
        var body = new StackPanel { Spacing = 10 };
        if (marks.Count > 0 || verification is not null)
        {
            body.Children.Add(RipMatchLine.BuildWithMarks(marks, verification));
        }
        if (records.Count > 0)
        {
            body.Children.Add(ReleaseRecordsRow.Build(records));
        }
        var flyout = new Flyout
        {
            Placement = PlacementMode.Bottom,
            Content = new Border
            {
                Padding = new Thickness(12, 10),
                Child = body,
            },
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
