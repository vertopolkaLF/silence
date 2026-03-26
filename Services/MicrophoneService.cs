using NAudio.CoreAudioApi;
using System;
using System.Collections.Generic;
using System.Linq;

namespace silence_.Services;

/// <summary>
/// Service for managing microphone mute state using Windows Core Audio API
/// </summary>
public class MicrophoneService : IDisposable
{
    private MMDeviceEnumerator? _deviceEnumerator;
    private MMDevice? _selectedDevice;
    private string? _selectedDeviceId;
    private bool _muteAllMicrophones;
    
    public event Action<bool>? MuteStateChanged;
    #pragma warning disable CS0067 // Event is never used - reserved for future use
    public event Action? DevicesChanged;
#pragma warning restore CS0067

    public const string ALL_MICROPHONES_ID = "__ALL_MICROPHONES__";

    public MicrophoneService()
    {
        _deviceEnumerator = new MMDeviceEnumerator();
    }

    /// <summary>
    /// Gets all available microphone devices
    /// </summary>
    public List<MicrophoneInfo> GetMicrophones()
    {
        var microphones = new List<MicrophoneInfo>();
        
        if (_deviceEnumerator == null) return microphones;

        try
        {
            var devices = _deviceEnumerator.EnumerateAudioEndPoints(DataFlow.Capture, DeviceState.Active);
            foreach (var device in devices)
            {
                microphones.Add(new MicrophoneInfo
                {
                    Id = device.ID,
                    Name = device.FriendlyName,
                    IsDefault = device.ID == GetDefaultMicrophoneId()
                });
            }
        }
        catch
        {
            // Shit happens, return empty list
        }

        return microphones;
    }

    /// <summary>
    /// Gets the default microphone device ID
    /// </summary>
    public string? GetDefaultMicrophoneId()
    {
        try
        {
            var defaultDevice = _deviceEnumerator?.GetDefaultAudioEndpoint(DataFlow.Capture, Role.Communications);
            return defaultDevice?.ID;
        }
        catch
        {
            return null;
        }
    }

    /// <summary>
    /// Selects a microphone by ID
    /// </summary>
    public void SelectMicrophone(string? deviceId)
    {
        _selectedDeviceId = deviceId;
        _muteAllMicrophones = deviceId == ALL_MICROPHONES_ID;
        UpdateSelectedDevice();
    }

    private void UpdateSelectedDevice()
    {
        _selectedDevice?.Dispose();
        _selectedDevice = null;

        if (string.IsNullOrEmpty(_selectedDeviceId) || _deviceEnumerator == null) return;
        
        // Skip device selection if "All microphones" is selected
        if (_muteAllMicrophones) return;

        try
        {
            _selectedDevice = _deviceEnumerator.GetDevice(_selectedDeviceId);
        }
        catch
        {
            // Device might not exist anymore, fuck it
        }
    }

    /// <summary>
    /// Gets the currently selected device, or default if none selected
    /// </summary>
    private MMDevice? GetActiveDevice()
    {
        if (_selectedDevice != null) return _selectedDevice;

        try
        {
            return _deviceEnumerator?.GetDefaultAudioEndpoint(DataFlow.Capture, Role.Communications);
        }
        catch
        {
            return null;
        }
    }

    /// <summary>
    /// Toggles mute state of the selected microphone
    /// </summary>
    public bool ToggleMute()
    {
        if (_muteAllMicrophones)
        {
            return ToggleMuteAllMicrophones();
        }

        var device = GetActiveDevice();
        if (device == null) return false;

        try
        {
            var newMuteState = !device.AudioEndpointVolume.Mute;
            device.AudioEndpointVolume.Mute = newMuteState;
            MuteStateChanged?.Invoke(newMuteState);
            return newMuteState;
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// Toggles mute state of all microphones
    /// </summary>
    private bool ToggleMuteAllMicrophones()
    {
        if (_deviceEnumerator == null) return false;

        try
        {
            var devices = _deviceEnumerator.EnumerateAudioEndPoints(DataFlow.Capture, DeviceState.Active);
            if (devices.Count == 0) return false;

            // Get the current state from the first device
            var currentState = devices[0].AudioEndpointVolume.Mute;
            var newMuteState = !currentState;

            // Apply to all devices
            foreach (var device in devices)
            {
                try
                {
                    device.AudioEndpointVolume.Mute = newMuteState;
                }
                catch
                {
                    // Skip devices that fail
                }
            }

            MuteStateChanged?.Invoke(newMuteState);
            return newMuteState;
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// Sets mute state directly
    /// </summary>
    public bool? SetMute(bool muted)
    {
        if (_muteAllMicrophones)
        {
            return SetMuteAllMicrophones(muted);
        }

        var device = GetActiveDevice();
        if (device == null) return null;

        try
        {
            if (device.AudioEndpointVolume.Mute == muted)
            {
                return muted;
            }

            device.AudioEndpointVolume.Mute = muted;
            MuteStateChanged?.Invoke(muted);
            return muted;
        }
        catch
        {
            return null;
        }
    }

    /// <summary>
    /// Sets mute state for all microphones
    /// </summary>
    private bool? SetMuteAllMicrophones(bool muted)
    {
        if (_deviceEnumerator == null) return null;

        try
        {
            var devices = _deviceEnumerator.EnumerateAudioEndPoints(DataFlow.Capture, DeviceState.Active);
            if (devices.Count == 0) return null;

            var currentState = devices[0].AudioEndpointVolume.Mute;
            if (currentState == muted)
            {
                return muted;
            }

            foreach (var device in devices)
            {
                try
                {
                    device.AudioEndpointVolume.Mute = muted;
                }
                catch
                {
                    // Skip devices that fail
                }
            }

            MuteStateChanged?.Invoke(muted);
            return muted;
        }
        catch
        {
            return null;
        }
    }

    /// <summary>
    /// Gets current mute state
    /// </summary>
    public bool IsMuted()
    {
        if (_muteAllMicrophones)
        {
            return GetMuteStateAllMicrophones();
        }

        var device = GetActiveDevice();
        if (device == null) return false;

        try
        {
            return device.AudioEndpointVolume.Mute;
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// Gets mute state for all microphones (returns true if any mic is muted)
    /// </summary>
    private bool GetMuteStateAllMicrophones()
    {
        if (_deviceEnumerator == null) return false;

        try
        {
            var devices = _deviceEnumerator.EnumerateAudioEndPoints(DataFlow.Capture, DeviceState.Active);
            if (devices.Count == 0) return false;

            // Return the state of the first device
            return devices[0].AudioEndpointVolume.Mute;
        }
        catch
        {
            return false;
        }
    }

    public string? SelectedDeviceId => _selectedDeviceId;

    public void Dispose()
    {
        _selectedDevice?.Dispose();
        _deviceEnumerator?.Dispose();
        _selectedDevice = null;
        _deviceEnumerator = null;
    }
}

public class MicrophoneInfo
{
    public required string Id { get; set; }
    public required string Name { get; set; }
    public bool IsDefault { get; set; }
}

