using Microsoft.Win32;
using System;
using System.IO;
using System.Text.Json;

namespace silence_.Services;

/// <summary>
/// Service for saving/loading settings and managing autostart
/// </summary>
public class SettingsService
{
    private const string RegistryRunKey = @"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
    private const string AppName = "silence!";
    
    private readonly string _settingsPath;
    private AppSettings _settings;

    public SettingsService()
    {
        var appDataPath = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "silence");
        
        Directory.CreateDirectory(appDataPath);
        _settingsPath = Path.Combine(appDataPath, "settings.json");
        
        _settings = LoadSettings();
    }

    public AppSettings Settings => _settings;

    private AppSettings LoadSettings()
    {
        try
        {
            if (File.Exists(_settingsPath))
            {
                var json = File.ReadAllText(_settingsPath);
                return JsonSerializer.Deserialize<AppSettings>(json) ?? new AppSettings();
            }
        }
        catch
        {
            // Settings file corrupted? Use defaults
        }

        return new AppSettings();
    }

    public void SaveSettings()
    {
        try
        {
            var json = JsonSerializer.Serialize(_settings, new JsonSerializerOptions 
            { 
                WriteIndented = true 
            });
            File.WriteAllText(_settingsPath, json);
        }
        catch
        {
            // Can't save settings
        }
    }

    public void SetAutoStart(bool enable)
    {
        _settings.AutoStartEnabled = enable;
        
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(RegistryRunKey, true);
            if (key == null) return;

            if (enable)
            {
                var exePath = Environment.ProcessPath;
                if (!string.IsNullOrEmpty(exePath))
                {
                    key.SetValue(AppName, $"\"{exePath}\" --minimized");
                }
            }
            else
            {
                key.DeleteValue(AppName, false);
            }
        }
        catch
        {
            // Registry access denied
        }

        SaveSettings();
    }

    public bool IsAutoStartEnabled()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(RegistryRunKey, false);
            return key?.GetValue(AppName) != null;
        }
        catch
        {
            return false;
        }
    }

    public void UpdateHotkey(int keyCode, ModifierKeys modifiers)
    {
        _settings.HotkeyCode = keyCode;
        _settings.HotkeyModifiers = modifiers;
        SaveSettings();
    }

    public void UpdateIgnoreModifiers(bool ignore)
    {
        _settings.IgnoreModifiers = ignore;
        SaveSettings();
    }

    public void UpdateSelectedMicrophone(string? deviceId)
    {
        _settings.SelectedMicrophoneId = deviceId;
        SaveSettings();
    }

    public void UpdateStartMinimized(bool minimized)
    {
        _settings.StartMinimized = minimized;
        SaveSettings();
    }

    public void UpdateCheckForUpdatesOnStartup(bool check)
    {
        _settings.CheckForUpdatesOnStartup = check;
        SaveSettings();
    }

    public void UpdateLastUpdateCheck()
    {
        _settings.LastUpdateCheck = DateTime.UtcNow;
        SaveSettings();
    }
    
    public void UpdateSoundsEnabled(bool enabled)
    {
        _settings.SoundsEnabled = enabled;
        SaveSettings();
    }
    
    public void UpdateMuteSound(string? preloadedKey, string? customPath)
    {
        _settings.MuteSoundPreloaded = preloadedKey;
        _settings.MuteSoundCustomPath = customPath;
        SaveSettings();
    }
    
    public void UpdateUnmuteSound(string? preloadedKey, string? customPath)
    {
        _settings.UnmuteSoundPreloaded = preloadedKey;
        _settings.UnmuteSoundCustomPath = customPath;
        SaveSettings();
    }
    
    public void UpdateSoundVolume(float volume)
    {
        _settings.SoundVolume = Math.Clamp(volume, 0f, 1f);
        SaveSettings();
    }
}

public class AppSettings
{
    public int HotkeyCode { get; set; } = 0x4D; // 'M' key
    public ModifierKeys HotkeyModifiers { get; set; } = ModifierKeys.Ctrl | ModifierKeys.Alt;
    public bool IgnoreModifiers { get; set; } = false; // Don't ignore modifiers for Ctrl+Alt+M
    public string? SelectedMicrophoneId { get; set; }
    public bool AutoStartEnabled { get; set; } = false;
    public bool StartMinimized { get; set; } = false; // Show settings window on first launch
    public bool CheckForUpdatesOnStartup { get; set; } = true; // Check for updates when app starts
    public DateTime? LastUpdateCheck { get; set; } // Last time we checked for updates
    
    // Sound settings
    public bool SoundsEnabled { get; set; } = false; // Sounds disabled by default
    public string? MuteSoundPreloaded { get; set; } = "sifi"; // Preloaded sound key (e.g., "sifi")
    public string? MuteSoundCustomPath { get; set; } // Custom sound file path (takes precedence)
    public string? UnmuteSoundPreloaded { get; set; } = "sifi"; // Preloaded sound key
    public string? UnmuteSoundCustomPath { get; set; } // Custom sound file path (takes precedence)
    public float SoundVolume { get; set; } = 0.5f; // Sound volume 0.0 - 1.0
}
