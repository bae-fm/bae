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

        var prefillWithTags = new CheckBox
        {
            Content = Loc.Chrome("settings.import.prefill_with_tags"),
        };
        prefillWithTags.IsCheckedChanged += (_, _) =>
        {
            if (_refreshingSettings)
            {
                return;
            }
            WriteSetting(
                () => _app.Settings.SetPrefillWithTags(
                    prefillWithTags.IsChecked == true),
                () => RenderCurrent(renderers));
        };
        content.Children.Add(prefillWithTags);
        content.Children.Add(SecondaryLabel(
            Loc.Chrome("settings.import.prefill_with_tags_help")));

        var identifyAutomatically = new CheckBox
        {
            Content = Loc.Chrome("settings.import.identify_automatically"),
        };
        identifyAutomatically.IsCheckedChanged += (_, _) =>
        {
            if (_refreshingSettings)
            {
                return;
            }
            WriteSetting(
                () => _app.Settings.SetIdentifyAutomatically(
                    identifyAutomatically.IsChecked == true),
                () => RenderCurrent(renderers));
        };
        content.Children.Add(identifyAutomatically);
        content.Children.Add(SecondaryLabel(
            Loc.Chrome("settings.import.identify_automatically_help")));

        // One checkbox per source, built as the settings arrive: which sources
        // exist is core's list, not a constant this file repeats.
        var sources = new StackPanel();
        content.Children.Add(sources);
        content.Children.Add(SecondaryLabel(Loc.Chrome("settings.import.sources_help")));

        renderers.Add(fresh =>
        {
            _refreshingSettings = true;
            prefillWithTags.IsChecked = fresh.PrefillWithTags;
            identifyAutomatically.IsChecked = fresh.IdentifyAutomatically;
            RenderSourceSwitches(sources, fresh, renderers);
            _refreshingSettings = false;
        });

        // The Discogs key belongs to the Discogs source switch above it: the
        // switch cannot be moved until a key is stored.
        BuildDiscogs(content, renderers);
    }

    /// <summary>Draw one checkbox per metadata source over the availability core
    /// computed. Whether a switch can be moved is core's answer too, covering
    /// both reasons it cannot: a source with no credential, which the switch
    /// cannot supply, and the only source still being asked, which core refuses
    /// to leave nothing behind.</summary>
    private void RenderSourceSwitches(
        StackPanel host,
        Settings fresh,
        List<Action<Settings>> renderers)
    {
        host.Children.Clear();
        foreach (var entry in fresh.MetadataSources)
        {
            var box = new CheckBox
            {
                Content = Loc.Chrome(
                    "settings.import.search_source",
                    "source",
                    BaeBridgeMethods.BridgeMetadataSourceName(entry.Source)),
                IsChecked = entry.Availability == BridgeSourceAvailability.On,
                IsEnabled = entry.CanChange,
            };
            var source = entry.Source;
            box.IsCheckedChanged += (_, _) =>
            {
                if (_refreshingSettings)
                {
                    return;
                }
                WriteSetting(
                    () => _app.Settings.SetMetadataSourceEnabled(
                        source, box.IsChecked == true),
                    () => RenderCurrent(renderers));
            };
            host.Children.Add(box);
        }
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
