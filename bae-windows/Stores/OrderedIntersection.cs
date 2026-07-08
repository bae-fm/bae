using System.Collections.Generic;
using System.Linq;

namespace Bae.Windows;

// Intersect several lists while keeping the first list's order. The storage sheet
// offers only the transitions every selected release allows, ordered by core's
// action list on the first release. Pure and generic, so it is unit-tested apart
// from the bridge storage types.
public static class OrderedIntersection
{
    public static List<T> Intersect<T>(IReadOnlyList<IReadOnlyList<T>> lists)
    {
        if (lists.Count == 0)
        {
            return new List<T>();
        }

        var common = new HashSet<T>(lists[0]);
        foreach (var list in lists.Skip(1))
        {
            common.IntersectWith(list);
        }

        // Order (and any duplicates) follow the first list; membership is the
        // intersection of them all.
        return lists[0].Where(common.Contains).ToList();
    }
}
