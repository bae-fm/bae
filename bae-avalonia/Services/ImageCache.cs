using System;
using System.Collections.Generic;

namespace Bae.Desktop;

/// <summary>Which decoded-image cache an entry lives in. Eviction never crosses
/// buckets, so a native-size release image cannot evict the album grid.</summary>
internal enum ImageBucket
{
    LibraryImage,
    ReleaseImage,
    Remote,
    LocalFile,
}

/// <summary>Per-kind byte budgets for the decoded cache.</summary>
internal sealed record ImageBudgets(
    int LibraryImage,
    int ReleaseImage,
    int Remote,
    int LocalFile)
{
    private const int Megabyte = 1024 * 1024;

    public static ImageBudgets Default { get; } = new(
        LibraryImage: 192 * Megabyte,
        ReleaseImage: 48 * Megabyte,
        Remote: 16 * Megabyte,
        LocalFile: 16 * Megabyte);

    internal int For(ImageBucket bucket) => bucket switch
    {
        ImageBucket.LibraryImage => LibraryImage,
        ImageBucket.ReleaseImage => ReleaseImage,
        ImageBucket.Remote => Remote,
        ImageBucket.LocalFile => LocalFile,
        _ => throw new ArgumentOutOfRangeException(nameof(bucket)),
    };
}

/// <summary>
/// The decoded-image cache behind <c>ImageStore</c>: four independent
/// least-recently-used buckets, each bounded by the decoded byte cost of what it
/// holds. Generic over the platform bitmap and free of both the generated
/// bindings and the UI toolkit, so the cache and eviction rules are exercised
/// directly by unit tests.
///
/// Entries are addressed by a content reference plus the pixel size it was
/// decoded at — see <see cref="ImageTokens"/>.
/// </summary>
internal sealed class ImageCache<TImage>
    where TImage : class
{
    private sealed class Entry(string key, TImage image, int cost)
    {
        internal string Key { get; } = key;
        internal TImage Image { get; } = image;
        internal int Cost { get; } = cost;
    }

    private sealed class Bucket(int budget)
    {
        internal int Budget { get; } = budget;
        internal Dictionary<string, LinkedListNode<Entry>> Index { get; } = new();

        /// Most recently used at the head; eviction takes from the tail.
        internal LinkedList<Entry> Recency { get; } = new();
        internal int Cost { get; set; }
    }

    private readonly Func<TImage, int> _costOf;
    private readonly Dictionary<ImageBucket, Bucket> _buckets;
    private readonly object _gate = new();

    internal ImageCache(Func<TImage, int> costOf, ImageBudgets? budgets = null)
    {
        _costOf = costOf;
        var limits = budgets ?? ImageBudgets.Default;
        _buckets = new Dictionary<ImageBucket, Bucket>();
        foreach (ImageBucket bucket in Enum.GetValues<ImageBucket>())
        {
            _buckets[bucket] = new Bucket(limits.For(bucket));
        }
    }

    /// <summary>The decode held under <paramref name="key"/>, or null when the
    /// bucket doesn't hold one. A hit is the most recently used entry.</summary>
    internal TImage? Get(ImageBucket bucket, string key)
    {
        lock (_gate)
        {
            var target = _buckets[bucket];
            if (!target.Index.TryGetValue(key, out var node))
            {
                return null;
            }

            target.Recency.Remove(node);
            target.Recency.AddFirst(node);
            return node.Value.Image;
        }
    }

    /// <summary>Hold <paramref name="image"/> under <paramref name="key"/>,
    /// evicting this bucket's least recently used entries until it is back inside
    /// its budget.</summary>
    internal void Store(ImageBucket bucket, string key, TImage image)
    {
        lock (_gate)
        {
            var target = _buckets[bucket];
            Drop(target, key);
            var node = target.Recency.AddFirst(new Entry(key, image, _costOf(image)));
            target.Index[key] = node;
            target.Cost += node.Value.Cost;
            EvictToBudget(target);
        }
    }

    private static void Drop(Bucket bucket, string key)
    {
        if (!bucket.Index.Remove(key, out var node))
        {
            return;
        }

        bucket.Recency.Remove(node);
        bucket.Cost -= node.Value.Cost;
    }

    private static void EvictToBudget(Bucket bucket)
    {
        while (bucket.Cost > bucket.Budget && bucket.Recency.Last is { } oldest)
        {
            Drop(bucket, oldest.Value.Key);
        }
    }
}
