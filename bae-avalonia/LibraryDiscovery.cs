using System;
using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Pre-library discovery over the native bridge: list the libraries on this
// device, and create a new one. Both the welcome window (before a library is
// open) and the open MainWindow (its libraries manager) need these, so they live
// here once rather than duplicated. Errors are logged and reported through the
// caller's status surface; a failure yields an empty list / a null id.
internal static class LibraryDiscovery
{
    internal static List<BridgeLibrary> Load(Action<string> reportError)
    {
        try
        {
            return NativeBae.Libraries();
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to discover libraries.", exception);
            reportError(exception.Message);
            return new List<BridgeLibrary>();
        }
    }

    internal static string? Create(Action<string> reportError)
    {
        try
        {
            return NativeBae.CreateLibrary();
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to create library.", exception);
            reportError(exception.Message);
            return null;
        }
    }
}
