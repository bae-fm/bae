using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;

namespace Bae.Desktop;

internal sealed class AppearanceSection : StackPanel
{
    private readonly AppearanceStore _appearance;
    private readonly Action<string> _showError;
    private readonly ComboBox _mode = new();
    private readonly ComboBox _accent = new();
    private readonly ComboBox _tone = new();
    private bool _rendering;

    internal AppearanceSection(AppearanceStore appearance, Action<string> showError)
    {
        _appearance = appearance;
        _showError = showError;
        Spacing = 10;
        Children.Add(SettingsWindow.SectionLabel(Loc.Chrome("appearance.title")));
        AddPicker(_mode, Loc.Chrome("appearance.mode"), Enum.GetValues<AppearanceMode>(), ModeLabel);
        AddPicker(_accent, Loc.Chrome("appearance.accent"), Enum.GetValues<AccentChoice>(), AccentLabel);
        AddPicker(_tone, Loc.Chrome("appearance.tone"), Enum.GetValues<SurfaceTone>(), ToneLabel);
        _mode.SelectionChanged += (_, _) => Save();
        _accent.SelectionChanged += (_, _) => Save();
        _tone.SelectionChanged += (_, _) => Save();
        AttachedToVisualTree += (_, _) =>
        {
            _appearance.Changed += Render;
            Render();
        };
        DetachedFromVisualTree += (_, _) => _appearance.Changed -= Render;
        Render();
    }

    private void AddPicker<T>(ComboBox picker, string title, IEnumerable<T> choices, Func<T, string> labelFor) where T : struct, Enum
    {
        picker.HorizontalAlignment = HorizontalAlignment.Stretch;
        foreach (var choice in choices)
        {
            var label = new TextBlock
            {
                Text = labelFor(choice),
                VerticalAlignment = VerticalAlignment.Center,
            };
            Control content = label;
            if (choice is AccentChoice accent)
            {
                content = new StackPanel
                {
                    Orientation = Orientation.Horizontal,
                    Spacing = 8,
                    Children =
                    {
                        new Border
                        {
                            Width = 16, Height = 16, CornerRadius = new CornerRadius(8),
                            Background = new SolidColorBrush(AppearancePalette.Bundled.AccentFill(accent)),
                        },
                        label,
                    },
                };
            }
            picker.Items.Add(new ComboBoxItem { Content = content, Tag = choice });
        }
        Avalonia.Automation.AutomationProperties.SetName(picker, title);
        Children.Add(new StackPanel
        {
            Spacing = 4,
            Children = { SettingsWindow.SecondaryLabel(title), picker },
        });
    }

    private static string ModeLabel(AppearanceMode value) => value switch
    {
        AppearanceMode.System => Loc.Chrome("appearance.system"),
        AppearanceMode.Light => Loc.Chrome("appearance.light"),
        AppearanceMode.Dark => Loc.Chrome("appearance.dark"),
        _ => throw new ArgumentOutOfRangeException(nameof(value)),
    };

    private static string AccentLabel(AccentChoice value) => value switch
    {
        AccentChoice.Blue => Loc.Chrome("appearance.blue"),
        AccentChoice.Indigo => Loc.Chrome("appearance.indigo"),
        AccentChoice.Purple => Loc.Chrome("appearance.purple"),
        AccentChoice.Pink => Loc.Chrome("appearance.pink"),
        AccentChoice.Red => Loc.Chrome("appearance.red"),
        AccentChoice.Amber => Loc.Chrome("appearance.amber"),
        AccentChoice.Green => Loc.Chrome("appearance.green"),
        AccentChoice.Teal => Loc.Chrome("appearance.teal"),
        _ => throw new ArgumentOutOfRangeException(nameof(value)),
    };

    private static string ToneLabel(SurfaceTone value) => value switch
    {
        SurfaceTone.Neutral => Loc.Chrome("appearance.neutral"),
        SurfaceTone.Slate => Loc.Chrome("appearance.slate"),
        SurfaceTone.Plum => Loc.Chrome("appearance.plum"),
        SurfaceTone.Midnight => Loc.Chrome("appearance.midnight"),
        SurfaceTone.Forest => Loc.Chrome("appearance.forest"),
        SurfaceTone.Sand => Loc.Chrome("appearance.sand"),
        _ => throw new ArgumentOutOfRangeException(nameof(value)),
    };

    private void Render()
    {
        _rendering = true;
        Select(_mode, _appearance.Current.Mode);
        Select(_accent, _appearance.Current.Accent);
        Select(_tone, _appearance.Current.Tone);
        _rendering = false;
    }

    private static void Select<T>(ComboBox picker, T value) where T : struct, Enum =>
        picker.SelectedItem = picker.Items.OfType<ComboBoxItem>().Single(item => Equals(item.Tag, value));

    private void Save()
    {
        if (_rendering)
        {
            return;
        }
        var next = new AppearancePreferences(
            (AppearanceMode)((ComboBoxItem)_mode.SelectedItem!).Tag!,
            (AccentChoice)((ComboBoxItem)_accent.SelectedItem!).Tag!,
            (SurfaceTone)((ComboBoxItem)_tone.SelectedItem!).Tag!);
        try
        {
            _appearance.Set(next);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            Render();
            _showError(Loc.Chrome("appearance.save_failed", "error", error.Message));
        }
    }
}
