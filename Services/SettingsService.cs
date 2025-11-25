using Microsoft.Win32;
using System;
using System.IO;
using System.Text.Json;

namespace Silence_.Services;

/// <summary>
/// Service for saving/loading settings and managing autostart
/// </summary>
public class SettingsService
{
    private const string RegistryRunKey = @"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
    private const string AppName = "Silence!";
    
    private readonly string _settingsPath;
    private AppSettings _settings;

    public SettingsService()
    {
        var appDataPath = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Silence");
        
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
}

public class AppSettings
{
    public int HotkeyCode { get; set; } = VirtualKeys.F21;
    public ModifierKeys HotkeyModifiers { get; set; } = ModifierKeys.None;
    public bool IgnoreModifiers { get; set; } = true;
    public string? SelectedMicrophoneId { get; set; }
    public bool AutoStartEnabled { get; set; } = false;
    public bool StartMinimized { get; set; } = true;
}
