using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia;
using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The export filename pattern editor: the pattern's tokens as removable chips in
// a field, and an "Add:" row offering the tokens the pattern doesn't use yet.
// Every edit reports the whole new token list through onChanged; the owner writes
// it and re-renders from the settings re-read — no optimistic mutation. Mirrors
// macOS's FilenameTokenEditor.
internal sealed class FilenameTokenEditor
{
    public Control View { get; }

    private readonly WrapPanel _chips = new();
    private readonly WrapPanel _addRow = new();
    private readonly Action<List<BridgeSaveFilenameToken>> _onChanged;
    private List<BridgeSaveFilenameToken> _tokens = new();

    public FilenameTokenEditor(Action<List<BridgeSaveFilenameToken>> onChanged)
    {
        _onChanged = onChanged;
        var box = new Border
        {
            Child = _chips,
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(4),
            Padding = new Thickness(6),
            MinHeight = 36,
        };
        box[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        View = new StackPanel { Spacing = 6, Children = { box, _addRow } };
    }

    public void Render(IReadOnlyList<BridgeSaveFilenameToken> tokens)
    {
        _tokens = tokens.ToList();

        _chips.Children.Clear();
        foreach (var token in _tokens)
        {
            _chips.Children.Add(Chip(token));
        }

        _addRow.Children.Clear();
        var available = SaveFilenameTokenDisplay.All
            .Where(token => !_tokens.Contains(token))
            .ToList();
        _addRow.IsVisible = available.Count > 0;
        if (available.Count == 0)
        {
            return;
        }
        var addLabel = new TextBlock
        {
            Text = Loc.Chrome("settings.formats.add_token"),
            VerticalAlignment = VerticalAlignment.Center,
            Margin = new Thickness(0, 0, 4, 0),
        };
        addLabel[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        _addRow.Children.Add(addLabel);
        foreach (var token in available)
        {
            _addRow.Children.Add(AddButton(token));
        }
    }

    // A chip: the token's label with a small dismiss glyph; clicking removes the
    // token from the pattern.
    private Button Chip(BridgeSaveFilenameToken token)
    {
        var label = SaveFilenameTokenDisplay.Label(token);
        var content = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            Children =
            {
                new TextBlock { Text = label, VerticalAlignment = VerticalAlignment.Center },
                new TextBlock { Text = "✕", FontSize = 10, VerticalAlignment = VerticalAlignment.Center },
            },
        };
        var chip = new Button { Content = content, Padding = new Thickness(9, 3, 7, 4), Margin = new Thickness(0, 0, 5, 5) };
        AutomationProperties.SetName(chip, Loc.Chrome("settings.formats.remove_token", "token", label));
        chip.Click += (_, _) => _onChanged(_tokens.Where(t => t != token).ToList());
        return chip;
    }

    // An add-row entry: clicking appends the token to the pattern.
    private Button AddButton(BridgeSaveFilenameToken token)
    {
        var button = new Button
        {
            Content = SaveFilenameTokenDisplay.Label(token),
            Padding = new Thickness(8, 2, 8, 3),
            Margin = new Thickness(0, 0, 5, 0),
            Background = Brushes.Transparent,
        };
        button.Click += (_, _) => _onChanged(_tokens.Append(token).ToList());
        return button;
    }
}
