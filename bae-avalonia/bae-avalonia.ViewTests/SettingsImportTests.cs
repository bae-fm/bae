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

        var pickers = content.GetLogicalDescendants().OfType<ComboBox>().ToList();
        Assert.Equal(2, pickers.Count);
        var picker = pickers[0];
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
        var modePicker = pickers[1];
        Assert.Equal(
            new[]
            {
                BridgeDefaultFindOnlineMode.Automatic,
                BridgeDefaultFindOnlineMode.SearchManually,
            },
            modePicker.Items.OfType<ComboBoxItem>()
                .Select(item => Assert.IsType<BridgeDefaultFindOnlineMode>(item.Tag)));
        Assert.Contains(
            content.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text
                == Loc.Chrome("settings.import.default_find_online_mode"));
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
                DefaultFindOnlineMode = BridgeDefaultFindOnlineMode.SearchManually,
            });
            Assert.True(modePicker.IsVisible);
            Assert.Equal(
                BridgeDefaultFindOnlineMode.SearchManually,
                Assert.IsType<ComboBoxItem>(modePicker.SelectedItem).Tag);
        }
    }
}
