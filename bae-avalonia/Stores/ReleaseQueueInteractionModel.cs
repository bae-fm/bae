using System.Collections.Generic;
using System.Linq;

namespace Bae.Desktop;

internal readonly record struct ReleaseGroupDisclosureKey(
    string WatchedRoot,
    string RelativePath);

internal sealed class ReleaseQueueInteractionModel
{
    private readonly Dictionary<ReleaseGroupDisclosureKey, bool> _expandedGroups = new();
    private readonly HashSet<string> _refreshingRoots = new();

    internal bool IsGroupExpanded(ReleaseGroupDisclosureKey key) =>
        !_expandedGroups.TryGetValue(key, out var expanded) || expanded;

    internal void SetGroupExpanded(ReleaseGroupDisclosureKey key, bool expanded) =>
        _expandedGroups[key] = expanded;

    internal void RetainGroupDisclosureKeys(
        IEnumerable<ReleaseGroupDisclosureKey> keys)
    {
        var current = keys.ToHashSet();
        foreach (var stale in _expandedGroups.Keys
            .Where(key => !current.Contains(key))
            .ToList())
        {
            _expandedGroups.Remove(stale);
        }
    }

    internal bool IsRefreshing(string root) => _refreshingRoots.Contains(root);

    internal void SetRefreshing(string root, bool refreshing)
    {
        if (refreshing)
        {
            _refreshingRoots.Add(root);
        }
        else
        {
            _refreshingRoots.Remove(root);
        }
    }
}

internal static class ReleaseQueueSortModel
{
    internal static List<T> Sort<T>(
        IEnumerable<T> entries,
        Func<T, string> title,
        bool descending) =>
        descending
            ? entries.OrderByDescending(title, StringComparer.CurrentCultureIgnoreCase).ToList()
            : entries.OrderBy(title, StringComparer.CurrentCultureIgnoreCase).ToList();
}

internal sealed class CoalescedReadModel
{
    private bool _running;
    private bool _dirty;

    internal bool Request()
    {
        if (_running)
        {
            _dirty = true;
            return false;
        }
        _running = true;
        return true;
    }

    internal bool Complete()
    {
        if (_dirty)
        {
            _dirty = false;
            return true;
        }
        _running = false;
        return false;
    }

    internal void Fail()
    {
        _running = false;
        _dirty = false;
    }
}

internal static class ReadySelectionModel
{
    internal static void Replace(HashSet<string> selection, IEnumerable<string> keys)
    {
        selection.Clear();
        selection.UnionWith(keys);
    }
}

internal static class CandidateSelectionModel
{
    internal static string? Retain(
        string? selectedKey,
        IReadOnlySet<string> candidateKeys) =>
        selectedKey is not null && candidateKeys.Contains(selectedKey)
            ? selectedKey
            : null;
}
