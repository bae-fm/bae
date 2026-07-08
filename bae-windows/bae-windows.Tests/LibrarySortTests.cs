using System.Linq;
using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

/// <summary>
/// Locks the library sort model: the album criteria's defaults, invariants
/// (unique fields, never empty), add/remove/reorder/direction/field manipulation,
/// the composer sort, and the JSON round-trip that persists the album sort like
/// macOS does.
/// </summary>
public sealed class LibrarySortTests
{
    [Fact]
    public void Albums_DefaultToDateAddedDescending()
    {
        var criteria = new AlbumSortCriteria();
        var only = Assert.Single(criteria.Items);
        Assert.Equal(AlbumSortField.DateAdded, only.Field);
        Assert.Equal(SortDirection.Descending, only.Direction);
    }

    [Fact]
    public void LibrarySort_DefaultsToAlbumsAndComposerNameAscending()
    {
        var sort = new LibrarySort();
        Assert.Equal(BrowserMode.Albums, sort.Mode);
        Assert.Equal(ComposerSortField.Name, sort.Composer.Field);
        Assert.Equal(SortDirection.Ascending, sort.Composer.Direction);
    }

    [Fact]
    public void Add_AppendsUnusedFieldAscending_AndIgnoresDuplicates()
    {
        var criteria = new AlbumSortCriteria();
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
        var criteria = new AlbumSortCriteria();
        Assert.Equal(
            new[] { AlbumSortField.Title, AlbumSortField.Artist, AlbumSortField.Year },
            criteria.AvailableToAdd);
    }

    [Fact]
    public void Remove_KeepsAtLeastOneCriterion()
    {
        var criteria = new AlbumSortCriteria();
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
        var criteria = new AlbumSortCriteria();
        criteria.Add(AlbumSortField.Title);

        criteria.SetDirection(AlbumSortField.Title, SortDirection.Descending);
        Assert.Equal(SortDirection.Descending, criteria.Items.Single(c => c.Field == AlbumSortField.Title).Direction);
        Assert.Equal(SortDirection.Descending, criteria.Items.Single(c => c.Field == AlbumSortField.DateAdded).Direction);
    }

    [Fact]
    public void SetField_SwapsInPlace_KeepingPositionAndDirection()
    {
        var criteria = new AlbumSortCriteria();
        criteria.SetDirection(AlbumSortField.DateAdded, SortDirection.Ascending);
        criteria.Add(AlbumSortField.Year);

        criteria.SetField(AlbumSortField.DateAdded, AlbumSortField.Title);

        Assert.Equal(
            new[] { AlbumSortField.Title, AlbumSortField.Year },
            criteria.Items.Select(c => c.Field));
        // Direction of the swapped slot is preserved.
        Assert.Equal(SortDirection.Ascending, criteria.Items[0].Direction);
    }

    [Fact]
    public void SetField_IgnoresATargetAlreadyInUse()
    {
        var criteria = new AlbumSortCriteria();
        criteria.Add(AlbumSortField.Year);

        criteria.SetField(AlbumSortField.DateAdded, AlbumSortField.Year);
        Assert.Equal(
            new[] { AlbumSortField.DateAdded, AlbumSortField.Year },
            criteria.Items.Select(c => c.Field));
    }

    [Fact]
    public void ChoosableFieldsFor_IsCurrentPlusUnused()
    {
        var criteria = new AlbumSortCriteria();
        criteria.Add(AlbumSortField.Artist);

        // DateAdded's picker offers itself plus the still-unused fields, never Artist.
        Assert.Equal(
            new[] { AlbumSortField.DateAdded, AlbumSortField.Title, AlbumSortField.Year },
            criteria.ChoosableFieldsFor(AlbumSortField.DateAdded));
    }

    [Fact]
    public void MoveUpDown_ReordersPriority_AndClampsAtEnds()
    {
        var criteria = new AlbumSortCriteria();
        criteria.Add(AlbumSortField.Title);
        criteria.Add(AlbumSortField.Year);

        criteria.MoveDown(AlbumSortField.DateAdded);
        Assert.Equal(
            new[] { AlbumSortField.Title, AlbumSortField.DateAdded, AlbumSortField.Year },
            criteria.Items.Select(c => c.Field));

        criteria.MoveUp(AlbumSortField.Year);
        Assert.Equal(
            new[] { AlbumSortField.Title, AlbumSortField.Year, AlbumSortField.DateAdded },
            criteria.Items.Select(c => c.Field));

        // First can't move up, last can't move down — both are no-ops.
        criteria.MoveUp(AlbumSortField.Title);
        criteria.MoveDown(AlbumSortField.DateAdded);
        Assert.Equal(
            new[] { AlbumSortField.Title, AlbumSortField.Year, AlbumSortField.DateAdded },
            criteria.Items.Select(c => c.Field));
    }

    [Fact]
    public void Changed_FiresOnRealMutationsOnly()
    {
        var criteria = new AlbumSortCriteria();
        var count = 0;
        criteria.Changed += () => count++;

        criteria.Add(AlbumSortField.Title); // fires
        criteria.Add(AlbumSortField.Title); // no-op, already present
        criteria.SetDirection(AlbumSortField.Title, SortDirection.Ascending); // no-op, already ascending
        criteria.MoveUp(AlbumSortField.DateAdded); // no-op, already first
        criteria.SetDirection(AlbumSortField.Title, SortDirection.Descending); // fires

        Assert.Equal(2, count);
    }

    [Fact]
    public void ToJson_MatchesTheMacOsPersistShape()
    {
        var criteria = new AlbumSortCriteria(new[]
        {
            new AlbumSortCriterion(AlbumSortField.Artist, SortDirection.Ascending),
            new AlbumSortCriterion(AlbumSortField.Year, SortDirection.Descending),
        });

        Assert.Equal(
            "[{\"field\":\"artist\",\"direction\":\"ascending\"},{\"field\":\"year\",\"direction\":\"descending\"}]",
            criteria.ToJson());
    }

    [Fact]
    public void FromJson_RoundTripsThroughToJson()
    {
        var original = new AlbumSortCriteria(new[]
        {
            new AlbumSortCriterion(AlbumSortField.Title, SortDirection.Descending),
            new AlbumSortCriterion(AlbumSortField.DateAdded, SortDirection.Ascending),
        });

        var restored = AlbumSortCriteria.FromJson(original.ToJson());
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
        var criteria = AlbumSortCriteria.FromJson(json);
        var only = Assert.Single(criteria.Items);
        Assert.Equal(AlbumSortField.DateAdded, only.Field);
        Assert.Equal(SortDirection.Descending, only.Direction);
    }

    [Fact]
    public void FromJson_SkipsUnknownAndDuplicateEntries()
    {
        var criteria = AlbumSortCriteria.FromJson(
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
    public void SetComposer_ReplacesTheWholeCriterion()
    {
        var sort = new LibrarySort();
        sort.SetComposer(ComposerSortField.WorkCount, SortDirection.Descending);
        Assert.Equal(ComposerSortField.WorkCount, sort.Composer.Field);
        Assert.Equal(SortDirection.Descending, sort.Composer.Direction);
    }

    [Fact]
    public void Vocab_PersistKeysAreStable()
    {
        Assert.Equal("dateAdded", LibrarySortVocab.PersistKey(AlbumSortField.DateAdded));
        Assert.Equal("title", LibrarySortVocab.PersistKey(AlbumSortField.Title));
        Assert.Equal("artist", LibrarySortVocab.PersistKey(AlbumSortField.Artist));
        Assert.Equal("year", LibrarySortVocab.PersistKey(AlbumSortField.Year));
        Assert.Equal("ascending", LibrarySortVocab.PersistKey(SortDirection.Ascending));
        Assert.Equal("descending", LibrarySortVocab.PersistKey(SortDirection.Descending));
    }
}
