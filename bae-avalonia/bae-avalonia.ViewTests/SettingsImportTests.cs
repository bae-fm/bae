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
            Render(renderers, new Settings
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

        Render(renderers, new Settings
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

        Render(renderers, new Settings
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

    /// The key belongs to the Discogs source switch, which cannot be moved
    /// until a key is stored — so it renders in the Import section under the
    /// switches, not in a section of its own.
    [AvaloniaFact]
    public void TheDiscogsKeyRendersAfterTheSourceSwitches()
    {
        var (content, _) = BuildImportSection();

        var children = content.Children.ToList();
        var sources = children.FindIndex(child => child is StackPanel);
        var tokenBox = children.FindIndex(
            child => child is TextBox box
                && box.Watermark
                    == Loc.Chrome("settings.discogs.token_placeholder"));
        Assert.True(sources >= 0);
        Assert.True(tokenBox > sources);

        Assert.Equal(
            new object?[]
            {
                Loc.Chrome("settings.discogs.save"),
                Loc.Chrome("settings.discogs.recheck"),
                Loc.Chrome("settings.discogs.remove"),
            },
            KeyButtons(content).Select(button => button.Content).ToArray());
    }

    /// Which of the key's controls show is the stored status, not a draft: no
    /// key offers the input and Save, a usable key offers Remove instead.
    [AvaloniaFact]
    public void TheDiscogsKeyControlsFollowTheStoredStatus()
    {
        var (content, renderers) = BuildImportSection();
        var tokenBox = content.Children.OfType<TextBox>().Single();
        var buttons = KeyButtons(content)
            .ToDictionary(button => (string)button.Content!);

        Render(renderers, new Settings { DiscogsStatus = "not_configured" });
        Assert.True(tokenBox.IsVisible);
        Assert.True(buttons[Loc.Chrome("settings.discogs.save")].IsVisible);
        Assert.False(buttons[Loc.Chrome("settings.discogs.remove")].IsVisible);

        Render(renderers, new Settings { DiscogsStatus = "valid", DiscogsUsable = true });
        Assert.False(tokenBox.IsVisible);
        Assert.False(buttons[Loc.Chrome("settings.discogs.save")].IsVisible);
        Assert.True(buttons[Loc.Chrome("settings.discogs.remove")].IsVisible);
    }

    /// A CheckBox is a Button in Avalonia, so the source switches would come
    /// back alongside the key's own buttons.
    private static List<Button> KeyButtons(StackPanel content) =>
        content.GetLogicalDescendants().OfType<Button>()
            .Where(button => button.GetType() == typeof(Button)).ToList();

    private static void Render(
        List<Action<Settings>> renderers,
        Settings settings)
    {
        foreach (var render in renderers)
        {
            render(settings);
        }
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
