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

        var source = new ComboBox { HorizontalAlignment = HorizontalAlignment.Stretch };
        AddSource(source, Loc.Chrome("settings.import.find_online"), BridgeDefaultImportMetadataSource.FindOnline);
        AddSource(source, Loc.Core("ui.import.metadata.file_tags"), BridgeDefaultImportMetadataSource.FileTags);
        AddSource(source, Loc.Chrome("settings.import.none"), BridgeDefaultImportMetadataSource.None);
        source.SelectionChanged += (_, _) =>
        {
            if (_refreshingSettings || source.SelectedItem is not ComboBoxItem { Tag: BridgeDefaultImportMetadataSource selected })
            {
                return;
            }
            WriteSetting(
                () => _app.Settings.SetDefaultImportMetadataSource(selected),
                () => RenderCurrent(renderers));
        };
        content.Children.Add(SecondaryLabel(Loc.Chrome("settings.import.default_source")));
        content.Children.Add(source);

        var automatic = new CheckBox
        {
            Content = Loc.Chrome("settings.import.automatic_identification"),
        };
        automatic.IsCheckedChanged += (_, _) =>
        {
            if (_refreshingSettings)
            {
                return;
            }
            WriteSetting(
                () => _app.Settings.SetAutomaticImportIdentification(
                    automatic.IsChecked == true),
                () => RenderCurrent(renderers));
        };
        content.Children.Add(automatic);
        var automaticHelp = SecondaryLabel(
            Loc.Chrome("settings.import.automatic_identification_help"));
        content.Children.Add(automaticHelp);

        renderers.Add(fresh =>
        {
            _refreshingSettings = true;
            SelectSource(source, fresh.DefaultImportMetadataSource);
            automatic.IsChecked = fresh.AutomaticImportIdentification;
            var showsAutomatic = fresh.DefaultImportMetadataSource
                == BridgeDefaultImportMetadataSource.FindOnline;
            automatic.IsVisible = showsAutomatic;
            automaticHelp.IsVisible = showsAutomatic;
            _refreshingSettings = false;
        });
    }

    private static void AddSource(
        ComboBox picker,
        string label,
        BridgeDefaultImportMetadataSource source) =>
        picker.Items.Add(new ComboBoxItem { Content = label, Tag = source });

    private static void SelectSource(
        ComboBox picker,
        BridgeDefaultImportMetadataSource selected)
    {
        foreach (var item in picker.Items)
        {
            if (item is ComboBoxItem { Tag: BridgeDefaultImportMetadataSource source }
                && source == selected)
            {
                picker.SelectedItem = item;
                return;
            }
        }
        throw new InvalidOperationException($"Unknown import metadata source: {selected}");
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
