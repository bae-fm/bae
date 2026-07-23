using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics;
using Windows.Storage;
using Windows.System;

namespace Bae.Windows;

// MainWindow: the library search flyout and its result-click handlers. Split
// out of MainWindow.xaml.cs unchanged.
public sealed partial class MainWindow : Window
{
    private void OnSearchSubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args)
    {
        if (CurrentHandleOrNull() == null)
        {
            return;
        }

        var query = args.QueryText?.Trim() ?? string.Empty;
        if (query.Length == 0)
        {
            SearchFlyout.Hide();
            LoadCurrentBrowserMode();
        }
        else
        {
            RenderSearch(_browser.Search(query));
        }
    }

    // Render the store's cover-attached results into the dropdown anchored under
    // the search field, over whatever browse pane is showing; a session closed
    // mid-search leaves the current view in place.
    private void RenderSearch(BrowserSearch search)
    {
        if (search.HandleGone)
        {
            return;
        }

        _browserPanes.RenderSearchResults(search.Results, search.Error);
        ShowSearchFlyout();
    }

    // Open the results dropdown under the field. Transient show mode keeps focus in
    // the search box (the default grab would pull it into the flyout); rows stay
    // clickable/tabbable.
    private void ShowSearchFlyout() =>
        SearchFlyout.ShowAt(SearchBox, new FlyoutShowOptions
        {
            ShowMode = FlyoutShowMode.Transient,
            Placement = FlyoutPlacementMode.BottomEdgeAlignedLeft,
        });

    // Re-show the dropdown when focus returns to a non-empty field that already has
    // results, so a click-away doesn't strand them. Guarded against re-showing when
    // it is already open (and the guard also keeps the Transient show from fighting
    // the focus event).
    private void OnSearchBoxGotFocus(object sender, RoutedEventArgs e)
    {
        if (!_searchFlyoutOpen && !string.IsNullOrEmpty(SearchBox.Text) && _browserPanes.HasResults)
        {
            ShowSearchFlyout();
        }
    }

    private async void OnComposerClick(object sender, ItemClickEventArgs e)
    {
        if (CurrentHandleOrNull() == null || e.ClickedItem is not ComposerSummary composer)
        {
            return;
        }

        await _browserPanes.ShowComposerDetail(composer.ArtistId);
    }

    private async void OnArtistClick(object sender, ItemClickEventArgs e)
    {
        if (CurrentHandleOrNull() == null || e.ClickedItem is not ArtistSummary artist)
        {
            return;
        }

        await _browserPanes.ShowArtistDetail(artist.ArtistId);
    }
}
