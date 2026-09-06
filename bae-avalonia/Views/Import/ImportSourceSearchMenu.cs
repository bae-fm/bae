using System;
using Avalonia.Controls;
using uniffi.bae_bridge;

namespace Bae.Desktop;

internal static class ImportSourceSearchMenu
{
    internal static Button Build(Action<BridgeMetadataSource> onSearch)
    {
        var menu = new MenuFlyout();
        foreach (var source in new[] { BridgeMetadataSource.MusicBrainz, BridgeMetadataSource.Discogs })
        {
            var item = new MenuItem
            {
                Header = Loc.Chrome("import.search.source", "source", BaeBridgeMethods.BridgeMetadataSourceName(source)),
            };
            item.Click += (_, _) => onSearch(source);
            menu.Items.Add(item);
        }
        var button = ImportPaneUi.RowButton(Loc.Chrome("action.search"));
        button.Flyout = menu;
        return button;
    }
}
