using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.LogicalTree;
using Avalonia.Threading;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class SettingsImportTests
{
    [AvaloniaFact]
    public void ImportSettingsKeepOnlineLookupIndependentOfDefaultSource()
    {
        var app = AppService.Stubbed(
            new SessionStore(Dispatcher.UIThread),
            Dispatcher.UIThread,
            new LibraryService());
        var settings = new SettingsWindow(
            app,
            new AppearanceStore(AppearancePreferences.Default, _ => { }),
            new UpdateService(),
            () => Task.CompletedTask,
            _ => Task.CompletedTask,
            () => Task.CompletedTask);
        var content = new StackPanel();
        var renderers = new List<Action<Settings>>();

        settings.BuildImport(content, renderers);

        var picker = Assert.Single(content.GetLogicalDescendants().OfType<ComboBox>());
        Assert.Equal(
            new[]
            {
                BridgeDefaultImportMetadataSource.FindOnline,
                BridgeDefaultImportMetadataSource.FileTags,
                BridgeDefaultImportMetadataSource.None,
            },
            picker.Items
                .OfType<ComboBoxItem>()
                .Select(item => Assert.IsType<BridgeDefaultImportMetadataSource>(item.Tag)));
        Assert.Contains(
            content.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == Loc.Chrome("settings.import.default_source"));
        var identifyAutomatically = Assert.Single(
            content.GetLogicalDescendants().OfType<CheckBox>());
        Assert.Equal(
            Loc.Chrome("settings.import.identify_automatically"),
            identifyAutomatically.Content);
        Assert.Contains(
            content.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text
                == Loc.Chrome("settings.import.identify_automatically_help"));
        Assert.Contains(
            content.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == Loc.Chrome("settings.import.online_lookup"));

        foreach (var source in new[]
        {
            BridgeDefaultImportMetadataSource.FindOnline,
            BridgeDefaultImportMetadataSource.FileTags,
            BridgeDefaultImportMetadataSource.None,
        })
        {
            Assert.Single(renderers)(new Settings
            {
                DefaultImportMetadataSource = source,
                IdentifyAutomatically = false,
            });
            Assert.False(identifyAutomatically.IsChecked);
        }
    }

    /// One checkbox per metadata source core reports, checked when the source
    /// is being asked. Whether a checkbox can be moved is core's answer, so the
    /// tab renders it rather than working out which source is the last one on.
    [AvaloniaFact]
    public void ImportSettingsDrawOneSwitchPerMetadataSource()
    {
        var (content, renderers) = BuildImportSection();

        Assert.Single(renderers)(new Settings
        {
            MetadataSources = new List<BridgeMetadataSourceSetting>
            {
                new(BridgeMetadataSource.MusicBrainz, BridgeSourceAvailability.Off, true),
                new(BridgeMetadataSource.Discogs, BridgeSourceAvailability.On, false),
            },
        });

        var boxes = content.GetLogicalDescendants().OfType<CheckBox>().ToList();
        Assert.Equal(3, boxes.Count);
        var musicBrainz = boxes[1];
        var discogs = boxes[2];
        Assert.Equal(
            Loc.Chrome(
                "settings.import.search_source",
                "source",
                BaeBridgeMethods.BridgeMetadataSourceName(
                    BridgeMetadataSource.MusicBrainz)),
            musicBrainz.Content);
        Assert.False(musicBrainz.IsChecked);
        Assert.True(musicBrainz.IsEnabled);
        // The only source still being asked: switching it off would leave
        // nothing to ask, so core says the switch cannot move.
        Assert.True(discogs.IsChecked);
        Assert.False(discogs.IsEnabled);
        Assert.Contains(
            content.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == Loc.Chrome("settings.import.sources_help"));
    }

    /// A source with no credential cannot be asked whatever the switch says, so
    /// its switch is off and cannot be moved.
    [AvaloniaFact]
    public void AnUnreachableSourceSwitchCannotBeMoved()
    {
        var (content, renderers) = BuildImportSection();

        Assert.Single(renderers)(new Settings
        {
            MetadataSources = new List<BridgeMetadataSourceSetting>
            {
                new(BridgeMetadataSource.MusicBrainz, BridgeSourceAvailability.On, false),
                new(
                    BridgeMetadataSource.Discogs,
                    BridgeSourceAvailability.NotConfigured,
                    false),
            },
        });

        var discogs = content.GetLogicalDescendants().OfType<CheckBox>().Last();
        Assert.False(discogs.IsChecked);
        Assert.False(discogs.IsEnabled);
    }

    private static (StackPanel Content, List<Action<Settings>> Renderers)
        BuildImportSection()
    {
        var app = AppService.Stubbed(
            new SessionStore(Dispatcher.UIThread),
            Dispatcher.UIThread,
            new LibraryService());
        var settings = new SettingsWindow(
            app,
            new AppearanceStore(AppearancePreferences.Default, _ => { }),
            new UpdateService(),
            () => Task.CompletedTask,
            _ => Task.CompletedTask,
            () => Task.CompletedTask);
        var content = new StackPanel();
        var renderers = new List<Action<Settings>>();
        settings.BuildImport(content, renderers);
        return (content, renderers);
    }
}
