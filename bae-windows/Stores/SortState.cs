using System;

namespace Bae.Windows;

// A library sort option: LabelKey is a chrome key resolved to the localized menu
// label at display time; Field is the locale-free sort identifier the generated
// bridge expects (never localized).
public sealed record SortOption(string LabelKey, string Field, bool Ascending);

// Which grid the library browser shows.
public enum BrowserMode
{
    Albums,
    Composers,
}

// The browser's sort selections: the current mode and the sort chosen per mode,
// kept apart so flipping between albums and composers restores each mode's own
// sort. The option tables pair the wire field the bridge sorts by with the chrome
// key for its menu label. No WinUI, no bridge types, so it is unit-tested; the
// store fetches with Active.Field/Ascending and the window fills the sort box from
// ActiveOptions/ActiveIndex.
public sealed class SortState
{
    public static readonly SortOption[] AlbumSortOptions =
    {
        new("sort.newest", "date_added", false),
        new("sort.title", "title", true),
        new("sort.artist", "artist", true),
        new("sort.year", "year", true),
    };

    public static readonly SortOption[] ComposerSortOptions =
    {
        new("sort.name", "name", true),
        new("search.section.works", "work_count", false),
        new("search.section.releases", "linked_release_count", false),
    };

    private SortOption _albumSort = AlbumSortOptions[0];
    private SortOption _composerSort = ComposerSortOptions[0];

    public BrowserMode Mode { get; private set; } = BrowserMode.Albums;

    public SortOption[] ActiveOptions =>
        Mode == BrowserMode.Composers ? ComposerSortOptions : AlbumSortOptions;

    public SortOption Active =>
        Mode == BrowserMode.Composers ? _composerSort : _albumSort;

    // The index the sort box should select for the active mode's current sort.
    // The active sort only ever comes from the mode's own table (OptionAt or
    // the initial element), so the lookup always finds it.
    public int ActiveIndex => Array.IndexOf(ActiveOptions, Active);

    public SortOption OptionAt(int index) => ActiveOptions[index];

    public void SetMode(BrowserMode mode) => Mode = mode;

    // Record the chosen sort for whichever mode is active, so the other mode's
    // sort is untouched.
    public void SetActive(SortOption sort)
    {
        if (Mode == BrowserMode.Composers)
        {
            _composerSort = sort;
        }
        else
        {
            _albumSort = sort;
        }
    }
}
