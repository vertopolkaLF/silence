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
    
    public void UpdateOverlayEnabled(bool enabled)
    {
        _settings.OverlayEnabled = enabled;
        SaveSettings();
    }
    
    public void UpdateOverlayVisibilityMode(string mode)
    {
        _settings.OverlayVisibilityMode = mode;
        SaveSettings();
    }
    
    public void UpdateOverlayScreen(string screenId)
    {
        _settings.OverlayScreenId = screenId;
        SaveSettings();
    }
    
    public void UpdateOverlayPosition(double percentX, double percentY)
    {
        _settings.OverlayPositionX = percentX;
        _settings.OverlayPositionY = percentY;
        SaveSettings();
    }
    
    public void UpdateOverlayShowText(bool showText)
    {
        _settings.OverlayShowText = showText;
        SaveSettings();
    }
    
    public void UpdateOverlayIconStyle(string style)
    {
        _settings.OverlayIconStyle = style;
        SaveSettings();
    }
    
    public void UpdateOverlayBackgroundStyle(string style)
    {
        _settings.OverlayBackgroundStyle = style;
        SaveSettings();
    }
    
    public void UpdateOverlayShowDuration(double duration)
    {
        _settings.OverlayShowDuration = Math.Clamp(duration, 0.1, 10.0);
        SaveSettings();
    }
    
    public void UpdateOverlayOpacity(int opacity)
    {
        _settings.OverlayOpacity = Math.Clamp(opacity, 0, 100);
        SaveSettings();
    }
    
    public void UpdateOverlayContentOpacity(int opacity)
    {
        _settings.OverlayContentOpacity = Math.Clamp(opacity, 20, 100);
        SaveSettings();
    }
    
    public void UpdateOverlayBorderRadius(int radius)
    {
        _settings.OverlayBorderRadius = Math.Clamp(radius, 0, 24);
        SaveSettings();
    }
    
    public void UpdateOverlayShowBorder(bool show)
    {
        _settings.OverlayShowBorder = show;
        SaveSettings();
    }
    
    public void UpdateOverlayScale(int scale)
    {
        _settings.OverlayScale = Math.Clamp(scale, 10, 200);
        SaveSettings();
    }
    
    public void UpdateOverlayVariant(string variant)
    {
        _settings.OverlayVariant = variant;
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
    
    // Overlay settings
    public bool OverlayEnabled { get; set; } = false; // Overlay disabled by default
    public string OverlayVisibilityMode { get; set; } = "WhenMuted"; // Always, WhenMuted, WhenUnmuted
    public string OverlayScreenId { get; set; } = "PRIMARY"; // PRIMARY or display ID
    public double OverlayPositionX { get; set; } = 50; // Position as percentage (0-100), 50 = center
    public double OverlayPositionY { get; set; } = 80; // Position as percentage (0-100), 80 = bottom 20%
    public bool OverlayShowText { get; set; } = false; // Show "Microphone is muted/unmuted" text
    public string OverlayIconStyle { get; set; } = "Colored"; // Colored, Monochrome
    public string OverlayBackgroundStyle { get; set; } = "Dark"; // Dark, Light
    public double OverlayShowDuration { get; set; } = 2.0; // Duration in seconds for "AfterToggle" mode (0.1 - 10.0)
    public int OverlayOpacity { get; set; } = 90; // Overlay background opacity (0-100%)
    public int OverlayContentOpacity { get; set; } = 100; // Overlay content (icon/text) opacity (20-100%)
    public int OverlayBorderRadius { get; set; } = 6; // Border radius in pixels (0-24)
    public bool OverlayShowBorder { get; set; } = true; // Show Win11 style border
    public int OverlayScale { get; set; } = 100; // Overlay size scale (10-200%)
    public string OverlayVariant { get; set; } = "MicIcon"; // MicIcon, Dot
}
