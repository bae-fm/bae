using System;
using Microsoft.UI.Xaml.Controls;

namespace Bae.Windows;

// The window shell's error banner state. The reducer and feature code write it
// through ShowBanner / ClearBanner; the window renders the InfoBar from it on
// Changed, so the banner has a single source.
internal sealed class ShellStore
{
    public bool BannerIsOpen { get; private set; }
    public InfoBarSeverity BannerSeverity { get; private set; }
    public string BannerTitle { get; private set; } = string.Empty;
    public string BannerMessage { get; private set; } = string.Empty;

    public event Action? Changed;

    public void ShowBanner(InfoBarSeverity severity, string title, string message)
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
