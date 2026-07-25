using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The settings window's Formats section: the format list (inline Save As… scope
// toggles, edit dialog, add/remove) and the default format per save target.
// Writes round-trip through config invalidation into the settings re-read (Render)
// with no optimistic mutation; preset edits send the whole set (set-state), never
// one mutated field. Mirrors macOS's FormatsSettingsTab.
internal sealed partial class FormatsSettingsSection
{
    public StackPanel View { get; } = new() { Spacing = 8 };

    private readonly AppService _app;
    private readonly Action<string> _showError;
    private readonly Action _clearError;
    private readonly Func<Func<Action, Control>, Task> _showDialog;

    private readonly ComboBox _defaultTrack = new() { HorizontalAlignment = HorizontalAlignment.Stretch };
    private readonly ComboBox _defaultRelease = new() { HorizontalAlignment = HorizontalAlignment.Stretch };
    private readonly StackPanel _presetPanel = new() { Spacing = 8 };

    private bool _rendering;

    // The settings snapshot the section last rendered — the list the preset
    // controls mutate and save whole. Set before the section is interactive
    // (the section renders once at open).
    private Settings? _current;

    public FormatsSettingsSection(
        AppService app,
        Action<string> showError,
        Action clearError,
        Func<Func<Action, Control>, Task> showDialog)
    {
        _app = app;
        _showError = showError;
        _clearError = clearError;
        _showDialog = showDialog;

        _defaultTrack.SelectionChanged += async (_, _) =>
            await SaveDefaultSelection(_defaultTrack, release: false);
        _defaultRelease.SelectionChanged += async (_, _) =>
            await SaveDefaultSelection(_defaultRelease, release: true);

        View.Children.Add(SectionLabel(Loc.Chrome("settings.formats.formats")));
        View.Children.Add(_presetPanel);
        View.Children.Add(AddPresetButton());
        View.Children.Add(Footer(Loc.Chrome("settings.formats.presets_footer")));
        View.Children.Add(SectionLabel(Loc.Chrome("settings.formats.defaults")));
        View.Children.Add(Footer(Loc.Chrome("settings.formats.defaults_footer")));
        View.Children.Add(LabeledColumn(Loc.Chrome("settings.formats.default_release_format"), _defaultRelease));
        View.Children.Add(LabeledColumn(Loc.Chrome("settings.formats.default_track_format"), _defaultTrack));
    }

    // Drive every control from the persisted settings. Called on open and on every
    // config-invalidation re-read; in-progress local drafts (a name being typed)
    // live in the rebuilt controls and re-seed from the fresh values.
    public void Render(Settings settings)
    {
        _current = settings;
        _rendering = true;
        PopulateSelection(_defaultTrack, settings, release: false);
        PopulateSelection(_defaultRelease, settings, release: true);
        RenderPresets(settings);
        _rendering = false;
    }

    // ── Shared chrome ────────────────────────────────────────────────────────

    private static TextBlock SectionLabel(string text)
    {
        var t = new TextBlock { Text = text, FontWeight = FontWeight.SemiBold, Margin = new Thickness(0, 8, 0, 0) };
        t[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        return t;
    }

    private static TextBlock Footer(string text)
    {
        var t = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap };
        t[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return t;
    }

    private static TextBlock PreviewLine()
    {
        var t = new TextBlock { TextWrapping = TextWrapping.Wrap };
        t[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return t;
    }

    private static Control LabeledColumn(string label, Control control)
    {
        var caption = new TextBlock { Text = label, FontSize = 12.5 };
        caption[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return new StackPanel { Spacing = 4, Children = { caption, control } };
    }

    // A justified editor row: the label leading, the control trailing.
    private static Grid LabeledRow(string label, Control control)
    {
        var row = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto") };
        var caption = new TextBlock { Text = label, VerticalAlignment = VerticalAlignment.Center };
        row.Children.Add(caption);
        Grid.SetColumn(control, 1);
        row.Children.Add(control);
        return row;
    }

    // Fill a tag-combo with (label, value) items and select the current one,
    // returning the item whose Tag matches so the caller can seed the selection.
    private static void FillTagCombo<T>(ComboBox combo, IEnumerable<(string Label, T Value)> items, T selected)
    {
        combo.Items.Clear();
        ComboBoxItem? selectedItem = null;
        foreach (var (label, value) in items)
        {
            var item = new ComboBoxItem { Content = label, Tag = value };
            combo.Items.Add(item);
            if (EqualityComparer<T>.Default.Equals(value, selected))
            {
                selectedItem = item;
            }
        }
        combo.SelectedItem = selectedItem;
    }

    // ── Default formats ──────────────────────────────────────────────────────

    private void PopulateSelection(ComboBox combo, Settings settings, bool release)
    {
        var selected = release ? settings.DefaultReleaseSavePreset : settings.DefaultTrackSavePreset;
        var presets = settings.SavePresets
            .Where(p => release ? p.AppliesToRelease : p.AppliesToTrack)
            .Select(p => (p.Name, p.Id));
        FillTagCombo(combo, presets, selected);
    }

    private async Task SaveDefaultSelection(ComboBox combo, bool release)
    {
        if (_rendering
            || combo.SelectedItem is not ComboBoxItem item
            || item.Tag is not string presetId)
        {
            return;
        }
        _clearError();
        var (current, error) = release
            ? await _app.Downloads.SetDefaultReleaseSavePreset(presetId)
            : await _app.Downloads.SetDefaultTrackSavePreset(presetId);
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            _showError(error);
            _app.SettingsStore.Reload();
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
    // Track/Release toggles set which Save As… scopes it appears under, and the
    // trailing minus removes it behind a confirmation. Mirrors macOS's PresetRow.
    private Grid PresetRow(Settings settings, SavePreset preset)
    {
        var row = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto,Auto"), ColumnSpacing = 12 };

        var open = new Button
        {
            Content = PresetHeader(preset),
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(4, 6),
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
            Content = new TextBlock { Text = "–", FontSize = 14 },
            Padding = new Thickness(8, 4, 8, 5),
            VerticalAlignment = VerticalAlignment.Center,
        };
        Avalonia.Automation.AutomationProperties.SetName(remove, Loc.Chrome("settings.formats.delete_preset"));
        remove.Click += async (_, _) => await ConfirmDeletePreset(settings, preset);
        Grid.SetColumn(remove, 2);
        row.Children.Add(remove);
        return row;
    }

    // The inline Track/Release scope toggles for a list row, writing the whole
    // updated preset through SavePresets. A single-file+CUE image is a
    // whole-release export, so the pregap choice fixes its scope: Track reads off,
    // Release reads on, and neither is editable — the same gating the editor's
    // scope checkboxes apply.
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
        async Task Save()
        {
            if (_rendering)
            {
                return;
            }
            preset.AppliesToTrack = track.IsChecked == true && !singleFileCue;
            preset.AppliesToRelease = release.IsChecked == true || singleFileCue;
            await SavePresets(settings.SavePresets);
        }
        track.IsCheckedChanged += async (_, _) => await Save();
        release.IsCheckedChanged += async (_, _) => await Save();

        return new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
            Children = { track, release },
        };
    }

    // The list row's label: name over a codec summary. The Track and Release
    // scopes it appears under are separate inline toggles in the row, not part of
    // this label.
    private static StackPanel PresetHeader(SavePreset preset)
    {
        var name = new TextBlock { Text = preset.Name, FontWeight = FontWeight.SemiBold };
        name[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var summary = new TextBlock { Text = PresetSummary(preset), FontSize = 12 };
        summary[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return new StackPanel { Spacing = 2, Children = { name, summary } };
    }

    private static string PresetSummary(SavePreset preset) => CodecLabel(preset.Codec);

    private (Grid Row, CheckBox Track, CheckBox Release) PresetScopeBoxes(Settings settings, SavePreset preset)
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
        async Task Save()
        {
            if (_rendering)
            {
                return;
            }
            preset.AppliesToTrack = track.IsChecked == true;
            preset.AppliesToRelease = release.IsChecked == true;
            await SavePresets(settings.SavePresets);
        }
        track.IsCheckedChanged += async (_, _) => await Save();
        release.IsCheckedChanged += async (_, _) => await Save();

        var boxes = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 12, Children = { track, release } };
        var row = LabeledRow(Loc.Chrome("settings.formats.show_in_menus"), boxes);
        return (row, track, release);
    }

    // ── Preset edit / delete dialogs (over the settings modal host) ──────────

    // The edit dialog. Every control inside writes through immediately (the dialog
    // renders from the same mutate-and-save closures the list uses), so Close is
    // the only button.
    private Task ShowPresetEditor(Settings settings, SavePreset preset) =>
        _showDialog(close =>
        {
            var editor = PresetEditor(settings, preset);
            editor.MinWidth = 420;
            var column = new StackPanel { Spacing = 12 };
            column.Children.Add(new TextBlock { Text = preset.Name, FontSize = 20, FontWeight = FontWeight.SemiBold });
            column.Children.Add(new ScrollViewer { Content = editor, MaxHeight = 520 });
            var closeButton = new Button { Content = Loc.Chrome("action.close") };
            closeButton.Click += (_, _) => close();
            column.Children.Add(DialogUi.Actions(closeButton));
            return column;
        });

    private Task ConfirmDeletePreset(Settings settings, SavePreset preset) =>
        _showDialog(close =>
        {
            var column = DialogUi.Column();
            column.Children.Add(DialogUi.Title(Loc.Chrome("settings.formats.delete_confirm", "name", preset.Name)));
            var cancel = new Button { Content = Loc.Chrome("action.cancel") };
            cancel.Click += (_, _) => close();
            var delete = DialogUi.Primary(Loc.Chrome("action.delete"));
            delete.Click += async (_, _) =>
            {
                close();
                settings.SavePresets.Remove(preset);
                await SavePresets(settings.SavePresets);
            };
            column.Children.Add(DialogUi.Actions(cancel, delete));
            return column;
        });
}
