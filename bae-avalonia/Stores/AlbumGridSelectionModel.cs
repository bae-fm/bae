using System;
using System.Collections.Generic;
using System.Linq;

namespace Bae.Desktop;

// Multi-selection state for the album grid: which albums are selected and the
// anchor a shift-click extends from. Plain BCL port of macOS's
// AlbumGridSelection — no WinUI, no bridge types — so it is linked into the
// test project. Owned by MainWindow as a browsing-session concern; after every
// mutation the window syncs Album.IsSelected over the loaded collection.
//
// "Selection" here is distinct from the one album whose detail dialog is open:
// multi-select is modifier-driven (ctrl/shift-click), and a plain click clears
// it.
public sealed class AlbumGridSelectionModel
{
    private readonly HashSet<string> _selectedIds = new();

    public IReadOnlySet<string> SelectedIds => _selectedIds;
    public string? AnchorId { get; private set; }

    public bool IsEmpty => _selectedIds.Count == 0;

    public bool Contains(string id) => _selectedIds.Contains(id);

    // Ctrl-click: toggle the id's membership and make it the new anchor.
    public void Toggle(string id)
    {
        if (!_selectedIds.Remove(id))
        {
            _selectedIds.Add(id);
        }
        AnchorId = id;
    }

    // Shift-click: union the contiguous range between the anchor and targetId
    // (in grid order) into the selection. Positions come from position; ids in
    // the range that aren't loaded (idAt returns null) are skipped. The anchor
    // is unchanged. If the anchor no longer resolves — a sort swapped the list
    // and its segment isn't fetched — this degrades to toggling targetId.
    public void ExtendRange(string targetId, Func<string, int?> position, Func<int, string?> idAt)
    {
        var anchorIndex = AnchorId is null ? null : position(AnchorId);
        var targetIndex = position(targetId);
        if (anchorIndex is not int anchor || targetIndex is not int target)
        {
            Toggle(targetId);
            return;
        }

        for (var index = Math.Min(anchor, target); index <= Math.Max(anchor, target); index++)
        {
            var id = idAt(index);
            if (id is not null)
            {
                _selectedIds.Add(id);
            }
        }
    }

    // Ctrl+A: select every loaded album id, anchored on the last.
    public void SelectAll(IReadOnlyList<string> ids)
    {
        _selectedIds.Clear();
        foreach (var id in ids)
        {
            _selectedIds.Add(id);
        }
        AnchorId = ids.Count > 0 ? ids[^1] : null;
    }

    public void Clear()
    {
        _selectedIds.Clear();
        AnchorId = null;
    }

    // Drop ids that no longer exist (a reload replaced the list). Clears the
    // anchor only if it was among them.
    public void Remove(IEnumerable<string> ids)
    {
        var removed = ids as ICollection<string> ?? ids.ToList();
        _selectedIds.ExceptWith(removed);
        if (AnchorId is not null && removed.Contains(AnchorId))
        {
            AnchorId = null;
        }
    }

    // The menu/drag target rule: a card that is part of a multi-selection
    // targets the whole selection in visible grid order (ids that don't
    // resolve to a position are dropped); any other card targets just itself.
    // Never mutates the selection — merely opening a menu or starting a drag
    // leaves it as is.
    public IReadOnlyList<string> OrderedTargets(string clickedId, Func<string, int?> position)
    {
        if (!_selectedIds.Contains(clickedId) || _selectedIds.Count <= 1)
        {
            return new[] { clickedId };
        }

        return _selectedIds
            .Select(id => (Id: id, Position: position(id)))
            .Where(entry => entry.Position is not null)
            .OrderBy(entry => entry.Position!.Value)
            .Select(entry => entry.Id)
            .ToList();
    }
}
