using System;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

/// <summary>A stand-in for the platform bitmap: the cache is generic over it and
/// only ever asks for its decoded byte cost.</summary>
internal sealed record FakeBitmap(string Name, int Cost);

public class ImageCacheTests
{
    private static ImageCache<FakeBitmap> Cache(ImageBudgets? budgets = null) =>
        new(bitmap => bitmap.Cost, budgets);

    private static ImageBudgets Budgets(
        int libraryImage = 1000,
        int releaseImage = 1000,
        int remote = 1000,
        int localFile = 1000) =>
        new(libraryImage, releaseImage, remote, localFile);

    // ── Storing and reading ────────────────────────────────────────────────────

    [Fact]
    public void Get_ReturnsWhatWasStoredAndNullForAnUnknownKey()
    {
        var cache = Cache();
        var image = new FakeBitmap("cover", 10);

        cache.Store(ImageBucket.LibraryImage, "k", image);

        Assert.Same(image, cache.Get(ImageBucket.LibraryImage, "k"));
        Assert.Null(cache.Get(ImageBucket.LibraryImage, "other"));
    }

    [Fact]
    public void Get_DoesNotReachAcrossBuckets()
    {
        var cache = Cache();
        cache.Store(ImageBucket.LibraryImage, "k", new FakeBitmap("cover", 10));

        Assert.Null(cache.Get(ImageBucket.ReleaseImage, "k"));
        Assert.Null(cache.Get(ImageBucket.Remote, "k"));
        Assert.Null(cache.Get(ImageBucket.LocalFile, "k"));
    }

    [Fact]
    public void Store_ReplacesTheEntryUnderTheSameKeyWithoutDoubleCounting()
    {
        var cache = Cache(Budgets(libraryImage: 100));
        cache.Store(ImageBucket.LibraryImage, "k", new FakeBitmap("first", 60));
        cache.Store(ImageBucket.LibraryImage, "k", new FakeBitmap("second", 60));

        // Double-counting would have pushed the bucket past its budget and
        // evicted the only entry it holds.
        Assert.Equal("second", cache.Get(ImageBucket.LibraryImage, "k")?.Name);
    }

    // ── Eviction ───────────────────────────────────────────────────────────────

    [Fact]
    public void Store_EvictsLeastRecentlyUsedOnceTheBucketIsOverBudget()
    {
        var cache = Cache(Budgets(libraryImage: 100));
        cache.Store(ImageBucket.LibraryImage, "a", new FakeBitmap("a", 50));
        cache.Store(ImageBucket.LibraryImage, "b", new FakeBitmap("b", 50));

        // Reading "a" makes "b" the least recently used, so "b" goes first.
        Assert.NotNull(cache.Get(ImageBucket.LibraryImage, "a"));
        cache.Store(ImageBucket.LibraryImage, "c", new FakeBitmap("c", 50));

        Assert.NotNull(cache.Get(ImageBucket.LibraryImage, "a"));
        Assert.Null(cache.Get(ImageBucket.LibraryImage, "b"));
        Assert.NotNull(cache.Get(ImageBucket.LibraryImage, "c"));
    }

    [Fact]
    public void Store_FillingOneBucketNeverEvictsAnother()
    {
        var cache = Cache(Budgets(libraryImage: 1000, releaseImage: 50));
        cache.Store(ImageBucket.LibraryImage, "cover", new FakeBitmap("cover", 40));

        // Far more release-image pressure than that bucket can hold.
        for (var index = 0; index < 32; index++)
        {
            cache.Store(
                ImageBucket.ReleaseImage,
                $"file-{index}",
                new FakeBitmap($"file-{index}", 40));
        }

        Assert.NotNull(cache.Get(ImageBucket.LibraryImage, "cover"));
    }

}

public class ImageTokensTests
{
    [Fact]
    public void Library_MovesWithTheContentVersion()
    {
        var first = ImageTokens.Library("Cover", "rel-1", "1");
        var second = ImageTokens.Library("Cover", "rel-1", "2");

        Assert.NotEqual(first, second);
        Assert.Equal(first, ImageTokens.Library("Cover", "rel-1", "1"));
    }

    [Fact]
    public void Library_SeparatesTheImageKinds()
    {
        Assert.NotEqual(
            ImageTokens.Library("Cover", "subject", "1"),
            ImageTokens.Library("Artist", "subject", "1"));
    }

    [Fact]
    public void ReleaseFile_KeysOnTheFileIdAlone()
    {
        Assert.Equal(ImageTokens.ReleaseFile("f-1"), ImageTokens.ReleaseFile("f-1"));
        Assert.NotEqual(ImageTokens.ReleaseFile("f-1"), ImageTokens.ReleaseFile("f-2"));
    }

    [Fact]
    public void Key_SeparatesTheSizesOneTokenIsDecodedAt()
    {
        var token = ImageTokens.Library("Cover", "rel-1", "1");

        Assert.NotEqual(ImageTokens.Key(token, 96), ImageTokens.Key(token, 240));
    }

    [Fact]
    public void LocalFile_KeysOnThePathAlone()
    {
        const string path = "/unread/path/candidate.png";

        Assert.Equal($"path:{path}", ImageTokens.LocalFile(path));
    }
}
