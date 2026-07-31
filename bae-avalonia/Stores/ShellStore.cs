using System;

namespace Bae.Desktop;

// The severity of the shell's error banner. A project-owned enum rather than a
// toolkit one, so the store carries no toolkit dependency; the window maps it to
// the banner's presentation.
internal enum BannerSeverity
{
    Informational,
    Success,
    Warning,
    Error,
}

// The window shell's error banner state. The reducer and feature code write it
// through ShowBanner / ClearBanner; the window renders the banner from it on
// Changed, so the banner has a single source.
internal sealed class ShellStore
{
    public bool BannerIsOpen { get; private set; }
    public BannerSeverity BannerSeverity { get; private set; }
    public string BannerTitle { get; private set; } = string.Empty;
    public string BannerMessage { get; private set; } = string.Empty;

    public event Action? Changed;

    public void ShowBanner(BannerSeverity severity, string title, string message)
    {
        BannerSeverity = severity;
        BannerTitle = title;
        BannerMessage = message;
        BannerIsOpen = true;
        Changed?.Invoke();
    }

    public void ClearBanner()
    {
        BannerIsOpen = false;
        Changed?.Invoke();
    }
}
