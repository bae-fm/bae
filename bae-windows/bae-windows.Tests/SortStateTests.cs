using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

/// <summary>
/// Locks the library browser's sort state: each mode keeps its own selection
/// across flips, the sort box's selected index resolves from the active sort, and
/// the option tables carry the wire sort fields (never localized labels).
/// </summary>
public sealed class SortStateTests
{
    [Fact]
    public void Defaults_ToAlbumsNewest()
    {
        var state = new SortState();
        Assert.Equal(BrowserMode.Albums, state.Mode);
        Assert.Same(SortState.AlbumSortOptions, state.ActiveOptions);
        Assert.Equal("date_added", state.Active.Field);
        Assert.Equal(0, state.ActiveIndex);
    }

    [Fact]
    public void ComposerMode_DefaultsToName()
    {
        var state = new SortState();
        state.SetMode(BrowserMode.Composers);
        Assert.Same(SortState.ComposerSortOptions, state.ActiveOptions);
        Assert.Equal("name", state.Active.Field);
        Assert.Equal(0, state.ActiveIndex);
    }

    [Fact]
    public void SetActive_PersistsPerModeAcrossFlips()
    {
        var state = new SortState();

        // Pick "artist" in albums, then "work_count" in composers.
        state.SetActive(state.OptionAt(2));
        Assert.Equal("artist", state.Active.Field);

        state.SetMode(BrowserMode.Composers);
        state.SetActive(state.OptionAt(1));
        Assert.Equal("work_count", state.Active.Field);

        // Flipping back restores each mode's own choice, not the other's.
        state.SetMode(BrowserMode.Albums);
        Assert.Equal("artist", state.Active.Field);
        Assert.Equal(2, state.ActiveIndex);

        state.SetMode(BrowserMode.Composers);
        Assert.Equal("work_count", state.Active.Field);
        Assert.Equal(1, state.ActiveIndex);
    }

    [Fact]
    public void ActiveIndex_ResolvesFromActiveSort()
    {
        var state = new SortState();
        state.SetActive(state.OptionAt(3));
        Assert.Equal(3, state.ActiveIndex);
        Assert.Equal("year", state.Active.Field);
    }

    [Fact]
    public void OptionTables_CarryWireFields()
    {
        Assert.Equal(
            new[] { "date_added", "title", "artist", "year" },
            System.Array.ConvertAll(SortState.AlbumSortOptions, o => o.Field));
        Assert.Equal(
            new[] { "name", "work_count", "linked_release_count" },
            System.Array.ConvertAll(SortState.ComposerSortOptions, o => o.Field));
    }

    [Fact]
    public void SetActive_OnlyTouchesTheActiveMode()
    {
        var state = new SortState();
        state.SetMode(BrowserMode.Composers);
        state.SetActive(state.OptionAt(2));
        Assert.Equal("linked_release_count", state.Active.Field);

        // Albums stayed on its default while composers changed.
        state.SetMode(BrowserMode.Albums);
        Assert.Equal("date_added", state.Active.Field);
    }
}
