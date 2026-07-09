using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;

namespace Bae.Windows;

// Which grid the library browser shows.
public enum BrowserMode
{
    Albums,
    Composers,
    Artists,
}

// The library album sort keys. Names match the bridge sort fields the album page
// is fetched with; the persistence and label vocabularies map off these.
public enum AlbumSortField
{
    DateAdded,
    Title,
    Artist,
    Year,
}

public enum ComposerSortField
{
    Name,
    WorkCount,
    LinkedReleaseCount,
}

public enum ArtistSortField
{
    Name,
    AlbumCount,
}

public enum SortDirection
{
    Ascending,
    Descending,
}

// The stable strings behind each sort enum: the chrome key its menu label
// resolves through, and (for album fields and directions) the locale-free key it
// serializes to on disk. Kept here, out of the bridge and out of the view, so the
// model owns the whole album-sort vocabulary and can be unit-tested.
public static class LibrarySortVocab
{
    // Canonical order for the add menu and field picker.
    public static readonly AlbumSortField[] AlbumFields =
    {
        AlbumSortField.DateAdded,
        AlbumSortField.Title,
        AlbumSortField.Artist,
        AlbumSortField.Year,
    };

    public static readonly ComposerSortField[] ComposerFields =
    {
        ComposerSortField.Name,
        ComposerSortField.WorkCount,
        ComposerSortField.LinkedReleaseCount,
    };

    public static readonly ArtistSortField[] ArtistFields =
    {
        ArtistSortField.Name,
        ArtistSortField.AlbumCount,
    };

    public static string LabelKey(AlbumSortField field) => field switch
    {
        AlbumSortField.DateAdded => "sort.dateAdded",
        AlbumSortField.Title => "sort.title",
        AlbumSortField.Artist => "sort.artist",
        AlbumSortField.Year => "sort.year",
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown album sort field"),
    };

    public static string LabelKey(ComposerSortField field) => field switch
    {
        ComposerSortField.Name => "sort.name",
        ComposerSortField.WorkCount => "search.section.works",
        ComposerSortField.LinkedReleaseCount => "search.section.releases",
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown composer sort field"),
    };

    public static string LabelKey(ArtistSortField field) => field switch
    {
        ArtistSortField.Name => "sort.name",
        ArtistSortField.AlbumCount => "search.section.albums",
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown artist sort field"),
    };

    // The chrome key naming the action of switching to a direction: the direction
    // toggle offers the opposite of what a criterion currently sorts by.
    public static string DirectionActionKey(SortDirection direction) => direction switch
    {
        SortDirection.Ascending => "sort.direction.ascending",
        SortDirection.Descending => "sort.direction.descending",
        _ => throw new ArgumentOutOfRangeException(nameof(direction), direction, "Unknown sort direction"),
    };

    public static SortDirection Opposite(SortDirection direction) =>
        direction == SortDirection.Ascending ? SortDirection.Descending : SortDirection.Ascending;

    public static string PersistKey(AlbumSortField field) => field switch
    {
        AlbumSortField.DateAdded => "dateAdded",
        AlbumSortField.Title => "title",
        AlbumSortField.Artist => "artist",
        AlbumSortField.Year => "year",
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown album sort field"),
    };

    public static string PersistKey(SortDirection direction) => direction switch
    {
        SortDirection.Ascending => "ascending",
        SortDirection.Descending => "descending",
        _ => throw new ArgumentOutOfRangeException(nameof(direction), direction, "Unknown sort direction"),
    };

    public static bool TryAlbumFieldFromPersistKey(string? key, out AlbumSortField field)
    {
        switch (key)
        {
            case "dateAdded":
                field = AlbumSortField.DateAdded;
                return true;
            case "title":
                field = AlbumSortField.Title;
                return true;
            case "artist":
                field = AlbumSortField.Artist;
                return true;
            case "year":
                field = AlbumSortField.Year;
                return true;
            default:
                field = default;
                return false;
        }
    }

    public static bool TryDirectionFromPersistKey(string? key, out SortDirection direction)
    {
        switch (key)
        {
            case "ascending":
                direction = SortDirection.Ascending;
                return true;
            case "descending":
                direction = SortDirection.Descending;
                return true;
            default:
                direction = default;
                return false;
        }
    }
}

// One album sort key and the direction it sorts in.
public sealed record AlbumSortCriterion(AlbumSortField Field, SortDirection Direction);

// The ordered album sort: a primary-first list of criteria, each field appearing
// at most once and the list never empty. All manipulation (add/remove and
// direction changes) and the on-disk round-trip live here so the view stays a
// thin renderer and the whole thing is unit-tested. Raises Changed on every
// real mutation so the persistence shell can write through.
public sealed class AlbumSortCriteria
{
    public static AlbumSortCriterion DefaultCriterion { get; } =
        new(AlbumSortField.DateAdded, SortDirection.Descending);

    private readonly List<AlbumSortCriterion> _criteria;

    public event Action? Changed;

    public AlbumSortCriteria()
        : this(new[] { DefaultCriterion })
    {
    }

    public AlbumSortCriteria(IEnumerable<AlbumSortCriterion> criteria)
    {
        _criteria = new List<AlbumSortCriterion>();
        foreach (var criterion in criteria)
        {
            // Drop a repeated field: a field sorts in one direction, so the second
            // occurrence carries no ordering the first didn't already impose.
            if (_criteria.All(existing => existing.Field != criterion.Field))
            {
                _criteria.Add(criterion);
            }
        }

        if (_criteria.Count == 0)
        {
            _criteria.Add(DefaultCriterion);
        }
    }

    public IReadOnlyList<AlbumSortCriterion> Items => _criteria;

    // Whether removing a criterion is allowed: the list must keep at least one.
    public bool CanRemove => _criteria.Count > 1;

    // Fields not yet in the list, in canonical order — the add menu's contents.
    public IReadOnlyList<AlbumSortField> AvailableToAdd =>
        LibrarySortVocab.AlbumFields.Where(field => !HasField(field)).ToList();

    public void Add(AlbumSortField field)
    {
        if (HasField(field))
        {
            return;
        }

        _criteria.Add(new AlbumSortCriterion(field, SortDirection.Ascending));
        Changed?.Invoke();
    }

    public void Remove(AlbumSortField field)
    {
        if (!CanRemove)
        {
            return;
        }

        var index = IndexOf(field);
        if (index < 0)
        {
            return;
        }

        _criteria.RemoveAt(index);
        Changed?.Invoke();
    }

    public void SetDirection(AlbumSortField field, SortDirection direction)
    {
        var index = IndexOf(field);
        if (index < 0 || _criteria[index].Direction == direction)
        {
            return;
        }

        _criteria[index] = _criteria[index] with { Direction = direction };
        Changed?.Invoke();
    }

    // Serialize to the same shape macOS persists: a JSON array of {field, direction}
    // objects keyed by the locale-free persist strings.
    public string ToJson()
    {
        var payload = _criteria.Select(criterion => new Dictionary<string, string>
        {
            ["field"] = LibrarySortVocab.PersistKey(criterion.Field),
            ["direction"] = LibrarySortVocab.PersistKey(criterion.Direction),
        });
        return JsonSerializer.Serialize(payload);
    }

    // Rebuild from persisted JSON. Lenient by design: an absent, empty, or
    // unparseable blob and any array that yields no known criteria fall back to the
    // default; individual entries with unknown keys are skipped.
    public static AlbumSortCriteria FromJson(string? json)
    {
        if (string.IsNullOrWhiteSpace(json))
        {
            return new AlbumSortCriteria();
        }

        try
        {
            using var document = JsonDocument.Parse(json);
            if (document.RootElement.ValueKind != JsonValueKind.Array)
            {
                return new AlbumSortCriteria();
            }

            var parsed = new List<AlbumSortCriterion>();
            foreach (var element in document.RootElement.EnumerateArray())
            {
                if (element.ValueKind != JsonValueKind.Object
                    || !element.TryGetProperty("field", out var fieldElement)
                    || !element.TryGetProperty("direction", out var directionElement))
                {
                    continue;
                }

                if (fieldElement.ValueKind != JsonValueKind.String
                    || directionElement.ValueKind != JsonValueKind.String)
                {
                    continue;
                }

                if (LibrarySortVocab.TryAlbumFieldFromPersistKey(fieldElement.GetString(), out var field)
                    && LibrarySortVocab.TryDirectionFromPersistKey(directionElement.GetString(), out var direction))
                {
                    parsed.Add(new AlbumSortCriterion(field, direction));
                }
            }

            return parsed.Count == 0 ? new AlbumSortCriteria() : new AlbumSortCriteria(parsed);
        }
        catch (JsonException)
        {
            return new AlbumSortCriteria();
        }
    }

    private bool HasField(AlbumSortField field) => IndexOf(field) >= 0;

    private int IndexOf(AlbumSortField field) => _criteria.FindIndex(criterion => criterion.Field == field);
}

// One composer sort key and direction. Unlike the album sort, this is a single
// criterion and is not persisted — it resets to name-ascending each launch.
public sealed record ComposerSortCriterion(ComposerSortField Field, SortDirection Direction);

// One artist sort key and direction. Like the composer sort: a single
// criterion, not persisted — it resets to name-ascending each launch.
public sealed record ArtistSortCriterion(ArtistSortField Field, SortDirection Direction);

// The browser's whole sort state: the current mode, the persisted album sort
// criteria, and the in-memory composer sort. No WinUI, no bridge types.
public sealed class LibrarySort
{
    public BrowserMode Mode { get; private set; } = BrowserMode.Albums;

    public AlbumSortCriteria Albums { get; }

    public ComposerSortCriterion Composer { get; private set; } =
        new(ComposerSortField.Name, SortDirection.Ascending);

    public ArtistSortCriterion Artist { get; private set; } =
        new(ArtistSortField.Name, SortDirection.Ascending);

    public LibrarySort()
        : this(new AlbumSortCriteria())
    {
    }

    public LibrarySort(AlbumSortCriteria albums)
    {
        Albums = albums;
    }

    public void SetMode(BrowserMode mode) => Mode = mode;

    public void SetComposer(ComposerSortField field, SortDirection direction) =>
        Composer = new ComposerSortCriterion(field, direction);

    public void SetArtist(ArtistSortField field, SortDirection direction) =>
        Artist = new ArtistSortCriterion(field, direction);
}
