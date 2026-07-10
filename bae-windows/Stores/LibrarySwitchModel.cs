using System.Collections.Generic;

namespace Bae.Windows;

// Maps a library-switch shortcut digit (Ctrl+1..9) onto the discovered-libraries
// list. No WinUI — the caller reduces its BridgeLibrary list to (id, isActive)
// pairs and applies the returned id.
public static class LibrarySwitchModel
{
    // digit is 1-based (Ctrl+1 -> 1). Returns the library id to switch to, or
    // null to no-op: digit outside 1..9, digit beyond the list, or the target is
    // already active.
    public static string? TargetLibraryId(
        IReadOnlyList<(string Id, bool IsActive)> libraries, int digit)
    {
        if (digit < 1 || digit > 9 || digit > libraries.Count)
        {
            return null;
        }

        var target = libraries[digit - 1];
        return target.IsActive ? null : target.Id;
    }
}
