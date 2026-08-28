using System;
using System.Collections.Generic;
using Avalonia.Controls;
using Avalonia.Layout;
using uniffi.bae_bridge;

namespace Bae.Desktop;

internal sealed partial class SettingsWindow
{
    internal void BuildImport(StackPanel content, List<Action<Settings>> renderers)
    {
        content.Children.Add(SectionLabel(Loc.Core("ui.import.metadata.title")));

        var mode = new ComboBox { HorizontalAlignment = HorizontalAlignment.Stretch };
        AddMode(mode, Loc.Core("ui.import.metadata.lookup"), BridgeDefaultImportMetadataMode.Lookup);
        AddMode(mode, Loc.Core("ui.import.metadata.file_tags"), BridgeDefaultImportMetadataMode.FileTags);
        AddMode(mode, Loc.Core("ui.import.metadata.manual"), BridgeDefaultImportMetadataMode.Manual);
        AddMode(mode, Loc.Chrome("settings.import.last_used"), BridgeDefaultImportMetadataMode.LastUsed);
        mode.SelectionChanged += (_, _) =>
        {
            if (_refreshingSettings || mode.SelectedItem is not ComboBoxItem { Tag: BridgeDefaultImportMetadataMode selected })
            {
                return;
            }
            WriteSetting(
                () => _app.Settings.SetDefaultImportMetadataMode(selected),
                () => RenderCurrent(renderers));
        };
        content.Children.Add(SecondaryLabel(Loc.Chrome("settings.import.open_unseeded")));
        content.Children.Add(mode);

        var automatic = new CheckBox
        {
            Content = Loc.Chrome("settings.import.automatic_lookup"),
        };
        automatic.IsCheckedChanged += (_, _) =>
        {
            if (_refreshingSettings)
            {
                return;
            }
            WriteSetting(
                () => _app.Settings.SetAutomaticImportMetadataLookup(
                    automatic.IsChecked == true),
                () => RenderCurrent(renderers));
        };
        content.Children.Add(automatic);
        content.Children.Add(SecondaryLabel(
            Loc.Chrome("settings.import.automatic_lookup_help")));

        renderers.Add(fresh =>
        {
            _refreshingSettings = true;
            SelectMode(mode, fresh.DefaultImportMetadataMode);
            automatic.IsChecked = fresh.AutomaticImportMetadataLookup;
            _refreshingSettings = false;
        });
    }

    private static void AddMode(
        ComboBox picker,
        string label,
        BridgeDefaultImportMetadataMode mode) =>
        picker.Items.Add(new ComboBoxItem { Content = label, Tag = mode });

    private static void SelectMode(
        ComboBox picker,
        BridgeDefaultImportMetadataMode selected)
    {
        foreach (var item in picker.Items)
        {
            if (item is ComboBoxItem { Tag: BridgeDefaultImportMetadataMode mode }
                && mode == selected)
            {
                picker.SelectedItem = item;
                return;
            }
        }
        throw new InvalidOperationException($"Unknown import metadata mode: {selected}");
    }

    private void WriteSetting(
        Func<(bool Current, string? Error)> write,
        Action restore)
    {
        ClearSettingsError();
        var (current, error) = write();
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            ShowSettingsError(error);
            restore();
        }
    }

    private void RenderCurrent(List<Action<Settings>> renderers)
    {
        if (_app.SettingsStore.Current is not { } current)
        {
            return;
        }
        foreach (var render in renderers)
        {
            render(current);
        }
    }
}
