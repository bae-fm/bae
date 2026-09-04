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
}
