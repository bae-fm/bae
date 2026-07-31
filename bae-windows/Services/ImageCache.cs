using System;
using System.Collections.Generic;

namespace Bae.Windows;

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
/// Entries are addressed by a token (what pins a decode to the exact bytes it
/// came from) plus the pixel size it was decoded at — see <see cref="ImageTokens"/>.
///
/// Provider art carries a second identity: the validator core returns with the
/// bytes. A fetch that reports a different validator for a URL invalidates every
/// decode taken from the previous one, at every size, not just the one being
/// reloaded.
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

    private sealed class RemoteEntry
    {
        internal string? Validator { get; set; }
        internal HashSet<string> Keys { get; } = new();
    }

    private readonly Func<TImage, int> _costOf;
    private readonly Dictionary<ImageBucket, Bucket> _buckets;
    private readonly Dictionary<string, RemoteEntry> _remote = new();
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

    /// <summary>Forget the decode held under <paramref name="key"/>, if any.</summary>
    internal void Remove(ImageBucket bucket, string key)
    {
        lock (_gate)
        {
            Drop(_buckets[bucket], key);
        }
    }

    /// <summary>Note that <paramref name="url"/>'s decode at some size lives
    /// under <paramref name="key"/>, so a later validator change can find it.</summary>
    internal void RecordRemoteKey(string url, string key)
    {
        lock (_gate)
        {
            RemoteEntryFor(url).Keys.Add(key);
        }
    }

    /// <summary>
    /// Adopt the validator a fetch just returned for <paramref name="url"/>. When
    /// it differs from the one the held decodes were made from, those decodes are
    /// stale at every size and are dropped. The first fetch for a URL establishes
    /// the validator without dropping anything.
    /// </summary>
    internal void AdoptRemoteValidator(string url, string validator)
    {
        lock (_gate)
        {
            var entry = RemoteEntryFor(url);
            if (entry.Validator is null || entry.Validator == validator)
            {
                entry.Validator = validator;
                return;
            }

            var remote = _buckets[ImageBucket.Remote];
            foreach (var key in entry.Keys)
            {
                Drop(remote, key);
            }

            entry.Keys.Clear();
            entry.Validator = validator;
        }
    }

    private RemoteEntry RemoteEntryFor(string url)
    {
        if (!_remote.TryGetValue(url, out var entry))
        {
            entry = new RemoteEntry();
            _remote[url] = entry;
        }

        return entry;
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
