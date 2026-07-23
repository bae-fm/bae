using System;
using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Cast state for the now-playing bar's cast control: the current cast status
// (which device, if any) and the discovered device list. Status is driven by the
// castStatusChanged UI event (so a receiver-side end updates it too); the device
// list by the castDevices projection while the picker is open. Discovery runs
// only while the picker is open.
internal sealed class CastStore
{
    private readonly SessionStore _session;

    // The devices found on the network, refreshed by the castDevices projection.
    public IReadOnlyList<BridgeCastDevice> Devices { get; private set; } = Array.Empty<BridgeCastDevice>();

    // The device name while casting, or null on local output.
    public string? CastingDeviceName { get; private set; }

    public bool IsCasting => CastingDeviceName is not null;

    // Fires when the cast status changes (active/inactive, device name).
    public event Action? StatusChanged;

    // Fires when the discovered device list changes.
    public event Action? DevicesChanged;

    public CastStore(SessionStore session)
    {
        _session = session;
    }

    // Apply a castStatusChanged event: the device name while casting, null back on
    // local output.
    public void ApplyStatus(string? deviceName)
    {
        CastingDeviceName = deviceName;
        StatusChanged?.Invoke();
    }

    // Reload the device list from the current handle (a castDevices invalidation).
    public void RefreshDevices()
    {
        var (current, devices) = _session.WithCurrentHandle(NativeBae.GetCastDevices);
        if (!current)
        {
            return;
        }
        Devices = devices;
        DevicesChanged?.Invoke();
    }

    // Begin/stop browsing (the picker opened/closed).
    public void StartDiscovery() => _session.WithCurrentHandle(NativeBae.StartCastDiscovery);

    public void StopDiscovery() => _session.WithCurrentHandle(NativeBae.StopCastDiscovery);

    // Switch playback to a device. Returns a localized error string on failure,
    // or null on success (or when there is no open library).
    public string? CastTo(string deviceId)
    {
        var (current, error) = _session.WithCurrentHandle(handle => NativeBae.CastTo(handle, deviceId));
        return current ? error : null;
    }

    // Stop casting and return playback to local output.
    public void StopCasting() => _session.WithCurrentHandle(NativeBae.StopCasting);
}
