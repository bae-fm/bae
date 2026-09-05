using System.Linq;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

/// <summary>
/// Locks the library sort model: the per-mode criteria's defaults, invariants
/// (unique fields, never empty), add/remove/direction manipulation, and the
/// JSON round-trip that persists all three sorts like macOS does.
/// </summary>
public sealed class LibrarySortTests
{
    [Fact]
    public void Albums_DefaultToDateAddedDescending()
    {
        var criteria = new SortCriteria<AlbumSortField>(LibrarySortVocab.Album);
        var only = Assert.Single(criteria.Items);
        Assert.Equal(AlbumSortField.DateAdded, only.Field);
        Assert.Equal(SortDirection.Descending, only.Direction);
    }

    [Fact]
    public void LibrarySort_DefaultsToAlbumsAndComposerNameAscending()
    {
        var sort = new LibrarySort();
        Assert.Equal(BrowserMode.Albums, sort.Mode);
        var only = Assert.Single(sort.Composers.Items);
        Assert.Equal(ComposerSortField.Name, only.Field);
        Assert.Equal(SortDirection.Ascending, only.Direction);
    }

    [Fact]
    public void LibrarySort_DefaultsArtistToNameAscending()
    {
        var sort = new LibrarySort();
        var only = Assert.Single(sort.Artists.Items);
        Assert.Equal(ArtistSortField.Name, only.Field);
        Assert.Equal(SortDirection.Ascending, only.Direction);
    }

    [Fact]
    public void Add_AppendsUnusedFieldAscending_AndIgnoresDuplicates()
    {
        var criteria = new SortCriteria<AlbumSortField>(LibrarySortVocab.Album);
        criteria.Add(AlbumSortField.Artist);

        Assert.Equal(
            new[] { AlbumSortField.DateAdded, AlbumSortField.Artist },
            criteria.Items.Select(c => c.Field));
        Assert.Equal(SortDirection.Ascending, criteria.Items[1].Direction);

        // A field already in the list can't be added again.
        criteria.Add(AlbumSortField.Artist);
        Assert.Equal(2, criteria.Items.Count);
    }

    [Fact]
    public void AvailableToAdd_ExcludesUsedFields_InCanonicalOrder()
    {
        var criteria = new SortCriteria<AlbumSortField>(LibrarySortVocab.Album);
        Assert.Equal(
            new[] { AlbumSortField.Title, AlbumSortField.Artist, AlbumSortField.Year },
            criteria.AvailableToAdd);
    }

    [Fact]
    public void ComposerCriteria_AvailableToAdd_InCanonicalOrder()
    {
        var criteria = new SortCriteria<ComposerSortField>(LibrarySortVocab.Composer);
        Assert.Equal(
            new[] { ComposerSortField.WorkCount, ComposerSortField.LinkedReleaseCount },
            criteria.AvailableToAdd);
    }

    [Fact]
    public void ArtistCriteria_AvailableToAdd_InCanonicalOrder()
    {
        var criteria = new SortCriteria<ArtistSortField>(LibrarySortVocab.Artist);
        Assert.Equal(new[] { ArtistSortField.AlbumCount }, criteria.AvailableToAdd);
    }

    [Fact]
    public void Remove_KeepsAtLeastOneCriterion()
    {
        var criteria = new SortCriteria<AlbumSortField>(LibrarySortVocab.Album);
        Assert.False(criteria.CanRemove);

        // Removing the last remaining criterion is refused.
        criteria.Remove(AlbumSortField.DateAdded);
        Assert.Single(criteria.Items);

        criteria.Add(AlbumSortField.Year);
        Assert.True(criteria.CanRemove);
        criteria.Remove(AlbumSortField.DateAdded);
        Assert.Equal(new[] { AlbumSortField.Year }, criteria.Items.Select(c => c.Field));
    }

    [Fact]
    public void SetDirection_ChangesOnlyTheNamedCriterion()
    {
        var criteria = new SortCriteria<AlbumSortField>(LibrarySortVocab.Album);
        criteria.Add(AlbumSortField.Title);

        criteria.SetDirection(AlbumSortField.Title, SortDirection.Descending);
        Assert.Equal(SortDirection.Descending, criteria.Items.Single(c => c.Field == AlbumSortField.Title).Direction);
        Assert.Equal(SortDirection.Descending, criteria.Items.Single(c => c.Field == AlbumSortField.DateAdded).Direction);
    }

    [Fact]
    public void SetField_RepointsInPlace_KeepingPositionAndDirection()
    {
        var criteria = new SortCriteria<AlbumSortField>(LibrarySortVocab.Album);
        criteria.Add(AlbumSortField.Title);
        criteria.SetDirection(AlbumSortField.Title, SortDirection.Descending);

        criteria.SetField(AlbumSortField.Title, AlbumSortField.Artist);

        Assert.Equal(
            new[] { AlbumSortField.DateAdded, AlbumSortField.Artist },
            criteria.Items.Select(c => c.Field).ToArray());
        Assert.Equal(SortDirection.Descending, criteria.Items[1].Direction);
    }

    [Fact]
    public void SetField_RefusesAFieldAlreadyInTheList()
    {
        var criteria = new SortCriteria<AlbumSortField>(LibrarySortVocab.Album);
        criteria.Add(AlbumSortField.Title);
        var count = 0;
        criteria.Changed += () => count++;

        criteria.SetField(AlbumSortField.Title, AlbumSortField.DateAdded); // taken by the other pill
        criteria.SetField(AlbumSortField.Title, AlbumSortField.Title); // no change
        criteria.SetField(AlbumSortField.Year, AlbumSortField.Artist); // not in the list

        Assert.Equal(0, count);
        Assert.Equal(
            new[] { AlbumSortField.DateAdded, AlbumSortField.Title },
            criteria.Items.Select(c => c.Field).ToArray());
    }

    [Fact]
    public void Changed_FiresOnRealMutationsOnly()
    {
        var criteria = new SortCriteria<AlbumSortField>(LibrarySortVocab.Album);
        var count = 0;
        criteria.Changed += () => count++;

        criteria.Add(AlbumSortField.Title); // fires
        criteria.Add(AlbumSortField.Title); // no-op, already present
        criteria.SetDirection(AlbumSortField.Title, SortDirection.Ascending); // no-op, already ascending
        criteria.SetDirection(AlbumSortField.Title, SortDirection.Descending); // fires

        Assert.Equal(2, count);
    }

    [Fact]
    public void ToJson_MatchesTheMacOsPersistShape()
    {
        var criteria = new SortCriteria<AlbumSortField>(LibrarySortVocab.Album, new[]
        {
            new SortCriterion<AlbumSortField>(AlbumSortField.Artist, SortDirection.Ascending),
            new SortCriterion<AlbumSortField>(AlbumSortField.Year, SortDirection.Descending),
        });

        Assert.Equal(
            "[{\"field\":\"artist\",\"direction\":\"ascending\"},{\"field\":\"year\",\"direction\":\"descending\"}]",
            criteria.ToJson());
    }

    [Fact]
    public void FromJson_RoundTripsThroughToJson()
    {
        var original = new SortCriteria<AlbumSortField>(LibrarySortVocab.Album, new[]
        {
            new SortCriterion<AlbumSortField>(AlbumSortField.Title, SortDirection.Descending),
            new SortCriterion<AlbumSortField>(AlbumSortField.DateAdded, SortDirection.Ascending),
        });

        var restored = SortCriteria<AlbumSortField>.FromJson(LibrarySortVocab.Album, original.ToJson());
        Assert.Equal(original.ToJson(), restored.ToJson());
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("   ")]
    [InlineData("not json")]
    [InlineData("{}")]
    [InlineData("[]")]
    [InlineData("[{\"field\":\"unknown\",\"direction\":\"ascending\"}]")]
    public void FromJson_FallsBackToDefault_OnMissingOrUnusable(string? json)
    {
        var criteria = SortCriteria<AlbumSortField>.FromJson(LibrarySortVocab.Album, json);
        var only = Assert.Single(criteria.Items);
        Assert.Equal(AlbumSortField.DateAdded, only.Field);
        Assert.Equal(SortDirection.Descending, only.Direction);
    }

    [Fact]
    public void FromJson_SkipsUnknownAndDuplicateEntries()
    {
        var criteria = SortCriteria<AlbumSortField>.FromJson(
            LibrarySortVocab.Album,
            "[{\"field\":\"artist\",\"direction\":\"ascending\"}," +
            "{\"field\":\"bogus\",\"direction\":\"ascending\"}," +
            "{\"field\":\"artist\",\"direction\":\"descending\"}," +
            "{\"field\":\"year\",\"direction\":\"nope\"}," +
            "{\"field\":\"title\",\"direction\":\"descending\"}]");

        Assert.Equal(
            new[] { AlbumSortField.Artist, AlbumSortField.Title },
            criteria.Items.Select(c => c.Field));
        Assert.Equal(SortDirection.Ascending, criteria.Items[0].Direction);
    }

    [Fact]
    public void ComposerCriteria_ToJson_UsesComposerPersistKeys()
    {
        var criteria = new SortCriteria<ComposerSortField>(LibrarySortVocab.Composer, new[]
        {
            new SortCriterion<ComposerSortField>(ComposerSortField.WorkCount, SortDirection.Descending),
            new SortCriterion<ComposerSortField>(ComposerSortField.LinkedReleaseCount, SortDirection.Ascending),
        });

        var json = criteria.ToJson();
        Assert.Equal(
            "[{\"field\":\"workCount\",\"direction\":\"descending\"},{\"field\":\"linkedReleaseCount\",\"direction\":\"ascending\"}]",
            json);

        var restored = SortCriteria<ComposerSortField>.FromJson(LibrarySortVocab.Composer, json);
        Assert.Equal(json, restored.ToJson());
    }

    [Fact]
    public void ArtistCriteria_JsonRoundTrips()
    {
        var original = new SortCriteria<ArtistSortField>(LibrarySortVocab.Artist, new[]
        {
            new SortCriterion<ArtistSortField>(ArtistSortField.AlbumCount, SortDirection.Descending),
            new SortCriterion<ArtistSortField>(ArtistSortField.Name, SortDirection.Ascending),
        });

        var restored = SortCriteria<ArtistSortField>.FromJson(LibrarySortVocab.Artist, original.ToJson());
        Assert.Equal(original.ToJson(), restored.ToJson());
    }

    [Fact]
    public void ComposerCriteria_FromJson_FallsBackToNameAscending()
    {
        foreach (var json in new[] { null, "garbage", "[{\"field\":\"unknown\",\"direction\":\"ascending\"}]" })
        {
            var criteria = SortCriteria<ComposerSortField>.FromJson(LibrarySortVocab.Composer, json);
            var only = Assert.Single(criteria.Items);
            Assert.Equal(ComposerSortField.Name, only.Field);
            Assert.Equal(SortDirection.Ascending, only.Direction);
        }
    }

    [Fact]
    public void Vocab_PersistKeysAreStable()
    {
        Assert.Equal("dateAdded", LibrarySortVocab.Album.PersistKey(AlbumSortField.DateAdded));
        Assert.Equal("title", LibrarySortVocab.Album.PersistKey(AlbumSortField.Title));
        Assert.Equal("artist", LibrarySortVocab.Album.PersistKey(AlbumSortField.Artist));
        Assert.Equal("year", LibrarySortVocab.Album.PersistKey(AlbumSortField.Year));
        Assert.Equal("ascending", LibrarySortVocab.PersistKey(SortDirection.Ascending));
        Assert.Equal("descending", LibrarySortVocab.PersistKey(SortDirection.Descending));

        Assert.Equal("name", LibrarySortVocab.Composer.PersistKey(ComposerSortField.Name));
        Assert.Equal("workCount", LibrarySortVocab.Composer.PersistKey(ComposerSortField.WorkCount));
        Assert.Equal("linkedReleaseCount", LibrarySortVocab.Composer.PersistKey(ComposerSortField.LinkedReleaseCount));

        Assert.Equal("name", LibrarySortVocab.Artist.PersistKey(ArtistSortField.Name));
        Assert.Equal("albumCount", LibrarySortVocab.Artist.PersistKey(ArtistSortField.AlbumCount));
    }
}
