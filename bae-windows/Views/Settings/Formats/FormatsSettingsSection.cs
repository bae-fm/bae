using System.Globalization;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;
using Windows.System;

namespace Bae.Windows;

// The settings dialog's Formats section: the format list (inline Save As…
// scope toggles, edit dialog, add/remove) and the default format per save
// target. Writes round-trip through config invalidation into the settings
// re-read (Render) with no optimistic mutation; preset edits send the whole
// set (set-state), never one mutated field. Mirrors macOS's
// FormatsSettingsTab.
internal sealed partial class FormatsSettingsSection
{
    public StackPanel View { get; } = new() { Spacing = 8 };

    private readonly SessionStore _session;
    private readonly SettingsStore _settings;
    private readonly Func<XamlRoot?> _dialogRoot;
    private readonly Action<string> _showError;
    private readonly Action _clearError;

    private readonly ComboBox _defaultTrack = new() { Header = Loc.Chrome("settings.formats.default_track_format") };
    private readonly ComboBox _defaultRelease = new() { Header = Loc.Chrome("settings.formats.default_release_format") };
    private readonly StackPanel _presetPanel = new() { Spacing = 8 };

    private bool _rendering;

    // The settings snapshot the section last rendered — the list the preset
    // controls mutate and save whole. Set before the section is interactive
    // (the dialog renders once at open).
    private Settings? _current;

    public FormatsSettingsSection(
        SessionStore session,
        SettingsStore settings,
        Func<XamlRoot?> dialogRoot,
        Action<string> showError,
        Action clearError)
    {
        _session = session;
        _settings = settings;
        _dialogRoot = dialogRoot;
        _showError = showError;
        _clearError = clearError;

        _defaultTrack.SelectionChanged += async (_, _) =>
            await SaveDefaultSelection(_defaultTrack, release: false);
        _defaultRelease.SelectionChanged += async (_, _) =>
            await SaveDefaultSelection(_defaultRelease, release: true);

        View.Children.Add(SectionLabel(Loc.Chrome("settings.formats.formats")));
        View.Children.Add(_presetPanel);
        View.Children.Add(AddPresetButton());
        View.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.formats.presets_footer"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });
        View.Children.Add(SectionLabel(Loc.Chrome("settings.formats.defaults")));
        View.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.formats.defaults_footer"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });
        View.Children.Add(_defaultRelease);
        View.Children.Add(_defaultTrack);
    }

    // Drive every control from the persisted settings. Called on open and on
    // every config-invalidation re-read; in-progress local drafts (a name being
    // typed) live in the rebuilt controls and re-seed from the fresh values.
    public void Render(Settings settings)
    {
        _current = settings;
        _rendering = true;
        PopulateSelection(_defaultTrack, settings, release: false);
        PopulateSelection(_defaultRelease, settings, release: true);
        RenderPresets(settings);
        _rendering = false;
    }

    private static TextBlock SectionLabel(string text) => new()
    {
        Text = text,
        FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        Margin = new Thickness(0, 8, 0, 0),
    };

    private static TextBlock PreviewLine() => new()
    {
        TextWrapping = TextWrapping.Wrap,
        Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
    };

    // ── Default formats ─────────────────────────────────────────────────────

    private void PopulateSelection(ComboBox combo, Settings settings, bool release)
    {
        combo.Items.Clear();
        var selected = release
            ? settings.DefaultReleaseSavePreset
            : settings.DefaultTrackSavePreset;
        foreach (var preset in settings.SavePresets.Where(
            p => release ? p.AppliesToRelease : p.AppliesToTrack))
        {
            combo.Items.Add(new ComboBoxItem
            {
                Content = preset.Name,
                Tag = preset.Id,
                IsSelected = preset.Id == selected,
            });
        }
    }

    private async System.Threading.Tasks.Task SaveDefaultSelection(ComboBox combo, bool release)
    {
        if (_rendering
            || combo.SelectedItem is not ComboBoxItem item
            || item.Tag is not string presetId)
        {
            return;
        }
        _clearError();
        var (current, error) = await _session.RunForCurrentHandle(handle => release
            ? NativeBae.SetDefaultReleaseSavePreset(handle, presetId)
            : NativeBae.SetDefaultTrackSavePreset(handle, presetId));
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            _showError(error);
            _settings.Reload();
        }
    }

    // ── Presets ──────────────────────────────────────────────────────────────

    private void RenderPresets(Settings settings)
    {
        _presetPanel.Children.Clear();
        foreach (var preset in settings.SavePresets)
        {
            _presetPanel.Children.Add(PresetRow(settings, preset));
        }
    }

    // One format in the list: the summary opens the edit dialog, the inline
    // Track/Release toggles set which Save As\u2026 scopes it appears under, and the
    // trailing minus removes it behind a confirmation. Mirrors macOS's
    // PresetRow.
    private Grid PresetRow(Settings settings, SavePreset preset)
    {
        var row = new Grid { ColumnSpacing = 12 };
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

        var open = new Button
        {
            Content = PresetHeader(preset),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
            Padding = new Thickness(4, 6, 4, 6),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
        };
        open.Click += async (_, _) => await ShowPresetEditor(settings, preset);
        row.Children.Add(open);

        var scopes = PresetScopeToggles(settings, preset);
        Grid.SetColumn(scopes, 1);
        row.Children.Add(scopes);

        var remove = new Button
        {
            // Segoe Fluent's Remove (minus) glyph.
            Content = new FontIcon { Glyph = "\uE738", FontSize = 12 },
            Padding = new Thickness(8, 4, 8, 5),
            VerticalAlignment = VerticalAlignment.Center,
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(
            remove, Loc.Chrome("settings.formats.delete_preset"));
        remove.Click += async (_, _) => await ConfirmDeletePreset(settings, preset);
        Grid.SetColumn(remove, 2);
        row.Children.Add(remove);
        return row;
    }

    // The inline Track/Release scope toggles for a list row, writing the whole
    // updated preset through SavePresets. A single-file+CUE image is a
    // whole-release export, so the pregap choice fixes its scope: Track reads
    // off, Release reads on, and neither is editable \u2014 the same gating the
    // editor's scope checkboxes apply.
    private StackPanel PresetScopeToggles(Settings settings, SavePreset preset)
    {
        var singleFileCue = preset.PregapPlacement == BridgeSavePregapPlacement.SingleFileWithCue;
        var track = new ToggleButton
        {
            Content = Loc.Chrome("settings.formats.preset_track"),
            IsChecked = !singleFileCue && preset.AppliesToTrack,
            IsEnabled = !singleFileCue,
        };
        var release = new ToggleButton
        {
            Content = Loc.Chrome("settings.formats.preset_release"),
            IsChecked = singleFileCue || preset.AppliesToRelease,
            IsEnabled = !singleFileCue,
        };
        async System.Threading.Tasks.Task Save()
        {
            if (_rendering)
            {
                return;
            }
            preset.AppliesToTrack = track.IsChecked == true && !singleFileCue;
            preset.AppliesToRelease = release.IsChecked == true || singleFileCue;
            await SavePresets(settings.SavePresets);
        }
        track.Checked += async (_, _) => await Save();
        track.Unchecked += async (_, _) => await Save();
        release.Checked += async (_, _) => await Save();
        release.Unchecked += async (_, _) => await Save();

        var panel = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
        };
        panel.Children.Add(track);
        panel.Children.Add(release);
        return panel;
    }

    // The edit dialog. Every control inside writes through immediately (the
    // dialog renders from the same mutate-and-save closures the expanders
    // used), so Close is the only button.
    private async System.Threading.Tasks.Task ShowPresetEditor(Settings settings, SavePreset preset)
    {
        if (_dialogRoot() is not { } root)
        {
            return;
        }
        var editor = PresetEditor(settings, preset);
        editor.MinWidth = 420;
        var dialog = new ContentDialog
        {
            Title = preset.Name,
            Content = new ScrollViewer { Content = editor },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = root,
        };
        await dialog.ShowAsync();
    }

    private async System.Threading.Tasks.Task ConfirmDeletePreset(Settings settings, SavePreset preset)
    {
        if (_dialogRoot() is not { } root)
        {
            return;
        }
        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("settings.formats.delete_confirm", "name", preset.Name),
            PrimaryButtonText = Loc.Chrome("action.delete"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = root,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }
        settings.SavePresets.Remove(preset);
        await SavePresets(settings.SavePresets);
    }

    // The list row's label: name over a codec summary. The Track and Release
    // scopes it appears under are separate inline toggles in the row, not part
    // of this label.
    private static StackPanel PresetHeader(SavePreset preset)
    {
        var titles = new StackPanel { Spacing = 2 };
        titles.Children.Add(new TextBlock
        {
            Text = preset.Name,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        titles.Children.Add(new TextBlock
        {
            Text = PresetSummary(preset),
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            FontSize = 12,
        });
        return titles;
    }

    private static string PresetSummary(SavePreset preset) => CodecLabel(preset.Codec);

    // A justified editor row: the label leading, the control trailing.
    private static Grid LabeledRow(string label, FrameworkElement control)
    {
        var row = new Grid();
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        row.Children.Add(new TextBlock
        {
            Text = label,
            VerticalAlignment = VerticalAlignment.Center,
        });
        Grid.SetColumn(control, 1);
        row.Children.Add(control);
        return row;
    }

    private (Grid Row, CheckBox Track, CheckBox Release) PresetScopeBoxes(
        Settings settings, SavePreset preset)
    {
        var track = new CheckBox
        {
            Content = Loc.Chrome("settings.formats.preset_track"),
            IsChecked = preset.AppliesToTrack,
        };
        var release = new CheckBox
        {
            Content = Loc.Chrome("settings.formats.preset_release"),
            IsChecked = preset.AppliesToRelease,
        };
        var singleFileCue = preset.PregapPlacement == BridgeSavePregapPlacement.SingleFileWithCue;
        track.IsEnabled = !singleFileCue;
        release.IsEnabled = !singleFileCue;
        async System.Threading.Tasks.Task Save()
        {
            if (_rendering)
            {
                return;
            }
            preset.AppliesToTrack = track.IsChecked == true;
            preset.AppliesToRelease = release.IsChecked == true;
            await SavePresets(settings.SavePresets);
        }
        track.Checked += async (_, _) => await Save();
        track.Unchecked += async (_, _) => await Save();
        release.Checked += async (_, _) => await Save();
        release.Unchecked += async (_, _) => await Save();

        var boxes = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 12 };
        boxes.Children.Add(track);
        boxes.Children.Add(release);
        var row = LabeledRow(Loc.Chrome("settings.formats.show_in_menus"), boxes);
        return (row, track, release);
    }

    // ── Codec display and switching ──────────────────────────────────────────

}
