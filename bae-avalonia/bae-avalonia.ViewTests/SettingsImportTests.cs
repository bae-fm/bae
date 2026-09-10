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
    /// Two settings, two switches, and neither write carries the other's
    /// value: the draft a candidate starts from and whether identification
    /// runs on its own are separate answers.
    [AvaloniaFact]
    public void ImportSettingsDrawTwoIndependentSwitches()
    {
        var (content, renderers) = BuildImportSection();

        var boxes = content.GetLogicalDescendants().OfType<CheckBox>().ToList();
        Assert.Equal(2, boxes.Count);
        Assert.Equal(Loc.Chrome("settings.import.prefill_with_tags"), boxes[0].Content);
        Assert.Equal(
            Loc.Chrome("settings.import.identify_automatically"),
            boxes[1].Content);
        foreach (var help in new[]
        {
            "settings.import.prefill_with_tags_help",
            "settings.import.identify_automatically_help",
        })
        {
            Assert.Contains(
                content.GetLogicalDescendants().OfType<TextBlock>(),
                text => text.Text == Loc.Chrome(help));
        }

        foreach (var (prefill, identify) in new[]
        {
            (true, false),
            (false, true),
            (false, false),
        })
        {
            Assert.Single(renderers)(new Settings
            {
                PrefillWithTags = prefill,
                IdentifyAutomatically = identify,
            });
            Assert.Equal(prefill, boxes[0].IsChecked);
            Assert.Equal(identify, boxes[1].IsChecked);
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
        Assert.Equal(4, boxes.Count);
        var musicBrainz = boxes[2];
        var discogs = boxes[3];
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
