using System;
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

    // The groups folded shut. Core needs them by name: a folded group's rows
    // are not in the list at all, so which ones are folded decides what sits at
    // every offset after them.
    internal IEnumerable<ReleaseGroupDisclosureKey> CollapsedKeys() =>
        _expandedGroups.Where(entry => !entry.Value).Select(entry => entry.Key).ToList();

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
