using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The export filename pattern editor: the pattern's tokens as removable chips
// in a field, and an "Add:" row offering the tokens the pattern doesn't use
// yet. Every edit reports the whole new token list through onChanged; the
// owner writes it through the bridge and re-renders from the settings re-read
// — no optimistic mutation. Mirrors macOS's FilenameTokenEditor.
internal sealed class FilenameTokenEditor
{
    public FrameworkElement View { get; }

    private readonly WrapPanel _chips = new() { Spacing = 5 };
    private readonly WrapPanel _addRow = new() { Spacing = 5 };
    private readonly Action<List<BridgeExportFilenameToken>> _onChanged;
    private List<BridgeExportFilenameToken> _tokens = new();

    public FilenameTokenEditor(Action<List<BridgeExportFilenameToken>> onChanged)
    {
        _onChanged = onChanged;
        var panel = new StackPanel { Spacing = 6 };
        panel.Children.Add(new Border
        {
            Child = _chips,
            BorderThickness = new Thickness(1),
            BorderBrush = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            CornerRadius = new CornerRadius(4),
            Padding = new Thickness(6),
            MinHeight = 36,
        });
        panel.Children.Add(_addRow);
        View = panel;
    }

    public void Render(IReadOnlyList<BridgeExportFilenameToken> tokens)
    {
        _tokens = tokens.ToList();

        _chips.Children.Clear();
        foreach (var token in _tokens)
        {
            _chips.Children.Add(Chip(token));
        }

        _addRow.Children.Clear();
        var available = ExportFilenameTokenDisplay.All
            .Where(token => !_tokens.Contains(token))
            .ToList();
        _addRow.Visibility = available.Count == 0 ? Visibility.Collapsed : Visibility.Visible;
        if (available.Count == 0)
        {
            return;
        }
        _addRow.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.export.add_token"),
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            VerticalAlignment = VerticalAlignment.Center,
        });
        foreach (var token in available)
        {
            _addRow.Children.Add(AddButton(token));
        }
    }

    // A chip: the token's label with a small dismiss glyph; clicking removes
    // the token from the pattern.
    private Button Chip(BridgeExportFilenameToken token)
    {
        var label = ExportFilenameTokenDisplay.Label(token);
        var content = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        content.Children.Add(new TextBlock { Text = label });
        content.Children.Add(new FontIcon
        {
            // Segoe Fluent's Cancel glyph.
            Glyph = "\uE711",
            FontSize = 8,
            VerticalAlignment = VerticalAlignment.Center,
        });
        var chip = new Button
        {
            Content = content,
            Padding = new Thickness(9, 3, 7, 4),
        };
        AutomationProperties.SetName(
            chip,
            Loc.Chrome("settings.export.remove_token", "token", label));
        chip.Click += (_, _) => _onChanged(_tokens.Where(t => t != token).ToList());
        return chip;
    }

    // An add-row entry: clicking appends the token to the pattern.
    private Button AddButton(BridgeExportFilenameToken token)
    {
        var button = new Button
        {
            Content = ExportFilenameTokenDisplay.Label(token),
            Padding = new Thickness(8, 2, 8, 3),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
        };
        button.Click += (_, _) => _onChanged(_tokens.Append(token).ToList());
        return button;
    }
}
