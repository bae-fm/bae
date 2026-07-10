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

// One sort key and the direction it sorts in.
public sealed record SortCriterion<TField>(TField Field, SortDirection Direction) where TField : struct, Enum;

// Everything mode-specific a criteria list needs: the canonical field order (the
// add menu's order), the default criterion, the chrome label key per field, and
// the locale-free persist-key round-trip. One instance per browser mode lives in
// LibrarySortVocab.
public sealed class SortVocab<TField> where TField : struct, Enum
{
    public IReadOnlyList<TField> CanonicalFields { get; }
    public SortCriterion<TField> DefaultCriterion { get; }
    public Func<TField, string> LabelKey { get; }
    public Func<TField, string> PersistKey { get; }
    public Func<string?, TField?> FieldFromPersistKey { get; }

    public SortVocab(
        IReadOnlyList<TField> canonicalFields,
        SortCriterion<TField> defaultCriterion,
        Func<TField, string> labelKey,
        Func<TField, string> persistKey,
        Func<string?, TField?> fieldFromPersistKey)
    {
        CanonicalFields = canonicalFields;
        DefaultCriterion = defaultCriterion;
        LabelKey = labelKey;
        PersistKey = persistKey;
        FieldFromPersistKey = fieldFromPersistKey;
    }
}

// The stable strings behind each sort enum: the chrome key its menu label
// resolves through, and the locale-free key it serializes to on disk. Kept
// here, out of the bridge and out of the view, so the model owns the whole
// sort vocabulary and can be unit-tested.
public static class LibrarySortVocab
{
    public static readonly SortVocab<AlbumSortField> Album = new(
        new[] { AlbumSortField.DateAdded, AlbumSortField.Title, AlbumSortField.Artist, AlbumSortField.Year },
        new SortCriterion<AlbumSortField>(AlbumSortField.DateAdded, SortDirection.Descending),
        LabelKey,
        PersistKey,
        AlbumFieldFromPersistKey);

    public static readonly SortVocab<ComposerSortField> Composer = new(
        new[] { ComposerSortField.Name, ComposerSortField.WorkCount, ComposerSortField.LinkedReleaseCount },
        new SortCriterion<ComposerSortField>(ComposerSortField.Name, SortDirection.Ascending),
        LabelKey,
        PersistKey,
        ComposerFieldFromPersistKey);

    public static readonly SortVocab<ArtistSortField> Artist = new(
        new[] { ArtistSortField.Name, ArtistSortField.AlbumCount },
        new SortCriterion<ArtistSortField>(ArtistSortField.Name, SortDirection.Ascending),
        LabelKey,
        PersistKey,
        ArtistFieldFromPersistKey);

    private static string LabelKey(AlbumSortField field) => field switch
    {
        AlbumSortField.DateAdded => "sort.dateAdded",
        AlbumSortField.Title => "sort.title",
        AlbumSortField.Artist => "sort.artist",
        AlbumSortField.Year => "sort.year",
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown album sort field"),
    };

    private static string LabelKey(ComposerSortField field) => field switch
    {
        ComposerSortField.Name => "sort.name",
        ComposerSortField.WorkCount => "search.section.works",
        ComposerSortField.LinkedReleaseCount => "search.section.releases",
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown composer sort field"),
    };

    private static string LabelKey(ArtistSortField field) => field switch
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

    private static string PersistKey(AlbumSortField field) => field switch
    {
        AlbumSortField.DateAdded => "dateAdded",
        AlbumSortField.Title => "title",
        AlbumSortField.Artist => "artist",
        AlbumSortField.Year => "year",
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown album sort field"),
    };

    private static string PersistKey(ComposerSortField field) => field switch
    {
        ComposerSortField.Name => "name",
        ComposerSortField.WorkCount => "workCount",
        ComposerSortField.LinkedReleaseCount => "linkedReleaseCount",
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown composer sort field"),
    };

    private static string PersistKey(ArtistSortField field) => field switch
    {
        ArtistSortField.Name => "name",
        ArtistSortField.AlbumCount => "albumCount",
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown artist sort field"),
    };

    public static string PersistKey(SortDirection direction) => direction switch
    {
        SortDirection.Ascending => "ascending",
        SortDirection.Descending => "descending",
        _ => throw new ArgumentOutOfRangeException(nameof(direction), direction, "Unknown sort direction"),
    };

    private static AlbumSortField? AlbumFieldFromPersistKey(string? key) => key switch
    {
        "dateAdded" => AlbumSortField.DateAdded,
        "title" => AlbumSortField.Title,
        "artist" => AlbumSortField.Artist,
        "year" => AlbumSortField.Year,
        _ => null,
    };

    private static ComposerSortField? ComposerFieldFromPersistKey(string? key) => key switch
    {
        "name" => ComposerSortField.Name,
        "workCount" => ComposerSortField.WorkCount,
        "linkedReleaseCount" => ComposerSortField.LinkedReleaseCount,
        _ => null,
    };

    private static ArtistSortField? ArtistFieldFromPersistKey(string? key) => key switch
    {
        "name" => ArtistSortField.Name,
        "albumCount" => ArtistSortField.AlbumCount,
        _ => null,
    };

    public static SortDirection? DirectionFromPersistKey(string? key) => key switch
    {
        "ascending" => SortDirection.Ascending,
        "descending" => SortDirection.Descending,
        _ => null,
    };
}

// The ordered sort for one browser mode: a primary-first list of criteria, each
// field appearing at most once and the list never empty. All manipulation
// (add/remove and direction changes) and the on-disk round-trip live here so the
// view stays a thin renderer and the whole thing is unit-tested. Raises Changed
// on every real mutation so the persistence shell can write through.
public sealed class SortCriteria<TField> where TField : struct, Enum
{
    private readonly List<SortCriterion<TField>> _criteria;

    public SortVocab<TField> Vocab { get; }

    public event Action? Changed;

    public SortCriteria(SortVocab<TField> vocab)
        : this(vocab, new[] { vocab.DefaultCriterion })
    {
    }

    public SortCriteria(SortVocab<TField> vocab, IEnumerable<SortCriterion<TField>> criteria)
    {
        Vocab = vocab;
        _criteria = new List<SortCriterion<TField>>();
        foreach (var criterion in criteria)
        {
            // Drop a repeated field: a field sorts in one direction, so the second
            // occurrence carries no ordering the first didn't already impose.
            if (_criteria.All(existing => !EqualityComparer<TField>.Default.Equals(existing.Field, criterion.Field)))
            {
                _criteria.Add(criterion);
            }
        }

        if (_criteria.Count == 0)
        {
            _criteria.Add(vocab.DefaultCriterion);
        }
    }

    public IReadOnlyList<SortCriterion<TField>> Items => _criteria;

    // Whether removing a criterion is allowed: the list must keep at least one.
    public bool CanRemove => _criteria.Count > 1;

    // Fields not yet in the list, in canonical order — the add menu's contents.
    public IReadOnlyList<TField> AvailableToAdd =>
        Vocab.CanonicalFields.Where(field => !HasField(field)).ToList();

    public void Add(TField field)
    {
        if (HasField(field))
        {
            return;
        }

        _criteria.Add(new SortCriterion<TField>(field, SortDirection.Ascending));
        Changed?.Invoke();
    }

    public void Remove(TField field)
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

    public void SetDirection(TField field, SortDirection direction)
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
            ["field"] = Vocab.PersistKey(criterion.Field),
            ["direction"] = LibrarySortVocab.PersistKey(criterion.Direction),
        });
        return JsonSerializer.Serialize(payload);
    }

    // Rebuild from persisted JSON. Lenient by design: an absent, empty, or
    // unparseable blob and any array that yields no known criteria fall back to the
    // default; individual entries with unknown keys are skipped.
    public static SortCriteria<TField> FromJson(SortVocab<TField> vocab, string? json)
    {
        if (string.IsNullOrWhiteSpace(json))
        {
            return new SortCriteria<TField>(vocab);
        }

        try
        {
            using var document = JsonDocument.Parse(json);
            if (document.RootElement.ValueKind != JsonValueKind.Array)
            {
                return new SortCriteria<TField>(vocab);
            }

            var parsed = new List<SortCriterion<TField>>();
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

                var field = vocab.FieldFromPersistKey(fieldElement.GetString());
                var direction = LibrarySortVocab.DirectionFromPersistKey(directionElement.GetString());
                if (field.HasValue && direction.HasValue)
                {
                    parsed.Add(new SortCriterion<TField>(field.Value, direction.Value));
                }
            }

            return parsed.Count == 0 ? new SortCriteria<TField>(vocab) : new SortCriteria<TField>(vocab, parsed);
        }
        catch (JsonException)
        {
            return new SortCriteria<TField>(vocab);
        }
    }

    private bool HasField(TField field) => IndexOf(field) >= 0;

    private int IndexOf(TField field) =>
        _criteria.FindIndex(criterion => EqualityComparer<TField>.Default.Equals(criterion.Field, field));
}

// The browser's whole sort state: the current mode and the persisted album,
// composer, and artist sort criteria. No WinUI, no bridge types.
public sealed class LibrarySort
{
    public BrowserMode Mode { get; private set; } = BrowserMode.Albums;

    public SortCriteria<AlbumSortField> Albums { get; }

    public SortCriteria<ComposerSortField> Composers { get; }

    public SortCriteria<ArtistSortField> Artists { get; }

    public LibrarySort()
        : this(
            new SortCriteria<AlbumSortField>(LibrarySortVocab.Album),
            new SortCriteria<ComposerSortField>(LibrarySortVocab.Composer),
            new SortCriteria<ArtistSortField>(LibrarySortVocab.Artist))
    {
    }

    public LibrarySort(
        SortCriteria<AlbumSortField> albums,
        SortCriteria<ComposerSortField> composers,
        SortCriteria<ArtistSortField> artists)
    {
        Albums = albums;
        Composers = composers;
        Artists = artists;
    }

    public void SetMode(BrowserMode mode) => Mode = mode;
}
