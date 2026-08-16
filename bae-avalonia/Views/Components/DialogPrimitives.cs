using System;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Platform.Storage;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Storage-domain dialog helpers shared by the storage sheet and the album-detail
// storage band: the resting/action labels (pure locale maps) and the make-local
// destination folder picker. The generic modal-host building blocks live in
// DialogUi; these carry the storage vocabulary both surfaces render.
internal static class DialogPrimitives
{
    // A release's resting storage state, mirroring macOS's precedence: local
    // first, then pinned, then cloud. Shown by the storage
    // row's Storage cell and the album-detail band's status line.
    internal static string RestingStorageLabel(bool isCloud, bool pinned)
    {
        if (!isCloud)
        {
            return Loc.Chrome("storage.state.local");
        }
        return pinned ? Loc.Chrome("storage.state.pinned") : Loc.Chrome("storage.state.cloud");
    }

    // The user-facing label for a storage transition, matching the macOS "Storage…"
    // sheet / context-menu wording. Shown by the storage row menu and the
    // album-detail band's action buttons.
    internal static string StorageActionLabel(BridgeReleaseStorageAction action) => action switch
    {
        BridgeReleaseStorageAction.MakeRemote => Loc.Chrome("storage.action.move_to_cloud"),
        BridgeReleaseStorageAction.MakeLocal => Loc.Chrome("storage.action.make_local"),
        BridgeReleaseStorageAction.Pin => Loc.Chrome("storage.action.pin"),
        BridgeReleaseStorageAction.Unpin => Loc.Chrome("storage.action.unpin"),
        _ => throw new ArgumentOutOfRangeException(nameof(action), action, "Unknown storage action"),
    };

    // The destination folder for a make-local transition. Returns the
    // chosen path, or null when the picker was cancelled or no window backs the
    // anchor. Shared by the storage row menu and the album-detail band so both take
    // the same destination for the transfer.
    internal static async Task<string?> PickLocalFolder(Visual anchor)
    {
        var storage = TopLevel.GetTopLevel(anchor)?.StorageProvider;
        if (storage is null)
        {
            return null;
        }
        var folders = await storage.OpenFolderPickerAsync(new FolderPickerOpenOptions { AllowMultiple = false });
        return folders.Count > 0 ? folders[0].TryGetLocalPath() : null;
    }
}
