using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Windows.System;

namespace Bae.Windows;

// LibraryBrowser: the library search field and its results dropdown. The query
// runs through the browser store; BrowserPanes renders the cover-attached rows
// into the flyout anchored under the field, over whatever browse pane is showing.
// The composer/artist list clicks open their detail through BrowserPanes too.
internal sealed partial class LibraryBrowser
{
    private void OnSearchSubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args)
    {
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }

        var query = args.QueryText?.Trim() ?? string.Empty;
        if (query.Length == 0)
        {
            _searchFlyout.Hide();
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
        _searchFlyout.ShowAt(_searchBox, new FlyoutShowOptions
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
        if (!_searchFlyoutOpen && !string.IsNullOrEmpty(_searchBox.Text) && _browserPanes.HasResults)
        {
            ShowSearchFlyout();
        }
    }

    // Escape clears a non-empty search box and drops back to the active browse
    // pane, standing in for a clear button (the AutoSuggestBox's query-icon slot
    // has no room for one).
    private void OnSearchBoxKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key == VirtualKey.Escape && !string.IsNullOrEmpty(_searchBox.Text))
        {
            _searchBox.Text = string.Empty;
            _searchFlyout.Hide();
            LoadCurrentBrowserMode();
            e.Handled = true;
        }
    }

    private async void OnComposerClick(object sender, ItemClickEventArgs e)
    {
        if (_session.CurrentHandleOrNull() == null || e.ClickedItem is not ComposerSummary composer)
        {
            return;
        }

        await _browserPanes.ShowComposerDetail(composer.ArtistId);
    }

    private async void OnArtistClick(object sender, ItemClickEventArgs e)
    {
        if (_session.CurrentHandleOrNull() == null || e.ClickedItem is not ArtistSummary artist)
        {
            return;
        }

        await _browserPanes.ShowArtistDetail(artist.ArtistId);
    }
}
