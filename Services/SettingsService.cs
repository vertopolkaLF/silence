using Microsoft.Win32;
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
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
        EnsureHotkeyBindingsInitialized();
        EnsureHoldHotkeyBindingsInitialized();
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

    public IReadOnlyList<HotkeyBindingSettings> GetHotkeyBindings()
    {
        EnsureHotkeyBindingsInitialized();
        return _settings.HotkeyBindings!
            .Select(binding => binding.Clone())
            .ToList();
    }

    public IReadOnlyList<HotkeyBindingSettings> GetHotkeyBindings(string action)
    {
        EnsureHotkeyBindingsInitialized();
        return _settings.HotkeyBindings!
            .Where(binding => string.Equals(binding.Action, action, StringComparison.OrdinalIgnoreCase))
            .Select(binding => binding.Clone())
            .ToList();
    }

    public HotkeyBindingSettings AddHotkeyBinding(string action)
    {
        EnsureHotkeyBindingsInitialized();

        var binding = CreateEmptyBinding(action);
        _settings.HotkeyBindings!.Add(binding);
        SyncLegacyHotkeyFields();
        SaveSettings();
        return binding.Clone();
    }

    public void UpdateHotkeyBinding(string bindingId, int keyCode, ModifierKeys modifiers)
    {
        EnsureHotkeyBindingsInitialized();

        var binding = GetHotkeyBindingById(bindingId);
        binding.KeyCode = keyCode;
        binding.Modifiers = modifiers;
        SyncLegacyHotkeyFields();
        SaveSettings();
    }

    public void UpdateHotkeyBindingIgnoreModifiers(string bindingId, bool ignore)
    {
        EnsureHotkeyBindingsInitialized();

        var binding = GetHotkeyBindingById(bindingId);
        binding.IgnoreModifiers = ignore;
        SyncLegacyHotkeyFields();
        SaveSettings();
    }

    public void RemoveHotkeyBinding(string bindingId)
    {
        EnsureHotkeyBindingsInitialized();

        var removed = _settings.HotkeyBindings!.RemoveAll(binding =>
            string.Equals(binding.Id, bindingId, StringComparison.OrdinalIgnoreCase));

        if (removed == 0)
        {
            return;
        }

        SyncLegacyHotkeyFields();
        SaveSettings();
    }

    public void UpdateHotkey(int keyCode, ModifierKeys modifiers)
    {
        var binding = GetOrCreateHotkeyBinding(HotkeyActions.Toggle);
        UpdateHotkeyBinding(binding.Id, keyCode, modifiers);
    }

    public void UpdateHoldHotkey(int keyCode, ModifierKeys modifiers)
    {
        var binding = GetOrCreateHoldHotkeyBinding();
        UpdateHoldHotkeyBinding(binding.Id, keyCode, modifiers);
    }

    public IReadOnlyList<HoldHotkeyBindingSettings> GetHoldHotkeyBindings()
    {
        EnsureHoldHotkeyBindingsInitialized();
        return _settings.HoldHotkeyBindings!
            .Select(binding => binding.Clone())
            .ToList();
    }

    public HoldHotkeyBindingSettings AddHoldHotkeyBinding()
    {
        EnsureHoldHotkeyBindingsInitialized();

        var binding = CreateEmptyHoldBinding();
        _settings.HoldHotkeyBindings!.Add(binding);
        SyncLegacyHoldHotkeyFields();
        SaveSettings();
        return binding.Clone();
    }

    public void UpdateHoldHotkeyBinding(string bindingId, int keyCode, ModifierKeys modifiers)
    {
        EnsureHoldHotkeyBindingsInitialized();

        var binding = GetHoldHotkeyBindingById(bindingId);
        binding.KeyCode = keyCode;
        binding.Modifiers = modifiers;
        SyncLegacyHoldHotkeyFields();
        SaveSettings();
    }

    public void RemoveHoldHotkeyBinding(string bindingId)
    {
        EnsureHoldHotkeyBindingsInitialized();

        var removed = _settings.HoldHotkeyBindings!.RemoveAll(binding =>
            string.Equals(binding.Id, bindingId, StringComparison.OrdinalIgnoreCase));

        if (removed == 0)
        {
            return;
        }

        SyncLegacyHoldHotkeyFields();
        SaveSettings();
    }

    public void UpdateIgnoreModifiers(bool ignore)
    {
        var binding = GetOrCreateHotkeyBinding(HotkeyActions.Toggle);
        UpdateHotkeyBindingIgnoreModifiers(binding.Id, ignore);
    }

    public void UpdateIgnoreHoldModifiers(bool ignore)
    {
        _settings.IgnoreHoldModifiers = ignore;
        SaveSettings();
    }

    public void UpdateHoldPlaySounds(bool playSounds)
    {
        _settings.HoldPlaySounds = playSounds;
        SaveSettings();
    }

    public void UpdateHoldShowOverlay(bool showOverlay)
    {
        _settings.HoldShowOverlay = showOverlay;
        SaveSettings();
    }

    public void UpdateHoldAction(string action)
    {
        _settings.HoldAction = action;
        SaveSettings();
    }

    public void UpdateHoldMuteSound(string? preloadedKey, string? customPath)
    {
        _settings.HoldMuteSoundPreloaded = preloadedKey;
        _settings.HoldMuteSoundCustomPath = customPath;
        SaveSettings();
    }

    public void UpdateHoldUnmuteSound(string? preloadedKey, string? customPath)
    {
        _settings.HoldUnmuteSoundPreloaded = preloadedKey;
        _settings.HoldUnmuteSoundCustomPath = customPath;
        SaveSettings();
    }

    public void UpdateHoldSoundVolume(float volume)
    {
        if (volume < 0)
        {
            _settings.HoldSoundVolume = null;
        }
        else
        {
            _settings.HoldSoundVolume = Math.Clamp(volume, 0f, 1f);
        }
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

    public void UpdateAutoMuteOnStartup(bool enabled)
    {
        _settings.AutoMuteOnStartup = enabled;
        SaveSettings();
    }

    public void UpdateAutoMuteAfterInactivityEnabled(bool enabled)
    {
        _settings.AutoMuteAfterInactivityEnabled = enabled;
        SaveSettings();
    }

    public void UpdateAutoMuteAfterInactivityMinutes(int minutes)
    {
        _settings.AutoMuteAfterInactivityMinutes = Math.Clamp(minutes, 1, 1440);
        SaveSettings();
    }

    public void UpdateAutoUnmuteOnActivity(bool enabled)
    {
        _settings.AutoUnmuteOnActivity = enabled;
        SaveSettings();
    }

    public void UpdateAutoMutePlaySounds(bool enabled)
    {
        _settings.AutoMutePlaySounds = enabled;
        SaveSettings();
    }

    public void UpdateTrayIconStyle(string style)
    {
        _settings.TrayIconStyle = style;
        SaveSettings();
    }

    public void UpdateLanguageOverride(string? language)
    {
        _settings.LanguageOverride = LocalizationService.ResolveAppLanguage(language);
        SaveSettings();
    }

    public string EnsureLanguageInitialized()
    {
        var resolvedLanguage = LocalizationService.ResolveAppLanguage(_settings.LanguageOverride);
        if (!string.Equals(_settings.LanguageOverride, resolvedLanguage, StringComparison.OrdinalIgnoreCase))
        {
            _settings.LanguageOverride = resolvedLanguage;
            SaveSettings();
        }

        return resolvedLanguage;
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
        _settings.OverlayScale = Math.Clamp(scale, 10, 400);
        SaveSettings();
    }
    
    public void UpdateOverlayVariant(string variant)
    {
        _settings.OverlayVariant = variant;
        SaveSettings();
    }

    public void UpdateOverlayButtonMode(bool enabled)
    {
        _settings.OverlayButtonMode = enabled;
        SaveSettings();
    }

    private void EnsureHotkeyBindingsInitialized()
    {
        var bindings = _settings.HotkeyBindings;
        var didChange = false;

        if (bindings == null)
        {
            bindings = new List<HotkeyBindingSettings>();
            _settings.HotkeyBindings = bindings;
            didChange = true;
        }

        var normalizedBindings = new List<HotkeyBindingSettings>();
        foreach (var binding in bindings)
        {
            if (binding == null)
            {
                didChange = true;
                continue;
            }

            if (!HotkeyActions.StandardActions.Contains(binding.Action, StringComparer.OrdinalIgnoreCase))
            {
                didChange = true;
                continue;
            }

            if (string.IsNullOrWhiteSpace(binding.Id))
            {
                binding.Id = CreateBindingId();
                didChange = true;
            }

            var normalizedAction = HotkeyActions.StandardActions.First(action =>
                string.Equals(action, binding.Action, StringComparison.OrdinalIgnoreCase));

            if (!string.Equals(binding.Action, normalizedAction, StringComparison.Ordinal))
            {
                binding.Action = normalizedAction;
                didChange = true;
            }

            normalizedBindings.Add(binding);
        }

        if (normalizedBindings.Count == 0)
        {
            normalizedBindings.Add(CreateDefaultBinding(HotkeyActions.Toggle));
            didChange = true;
        }

        _settings.HotkeyBindings = normalizedBindings;
        SyncLegacyHotkeyFields();

        if (didChange)
        {
            SaveSettings();
        }
    }

    private void EnsureHoldHotkeyBindingsInitialized()
    {
        var bindings = _settings.HoldHotkeyBindings;
        var didChange = false;

        if (bindings == null)
        {
            bindings = new List<HoldHotkeyBindingSettings>();
            _settings.HoldHotkeyBindings = bindings;
            didChange = true;
        }

        var normalizedBindings = new List<HoldHotkeyBindingSettings>();
        foreach (var binding in bindings)
        {
            if (binding == null)
            {
                didChange = true;
                continue;
            }

            if (string.IsNullOrWhiteSpace(binding.Id))
            {
                binding.Id = CreateBindingId();
                didChange = true;
            }

            normalizedBindings.Add(binding);
        }

        if (normalizedBindings.Count == 0 &&
            (_settings.HoldHotkeyCode > 0 || _settings.HoldHotkeyModifiers != ModifierKeys.None))
        {
            normalizedBindings.Add(new HoldHotkeyBindingSettings
            {
                Id = CreateBindingId(),
                KeyCode = _settings.HoldHotkeyCode,
                Modifiers = _settings.HoldHotkeyModifiers
            });
            didChange = true;
        }

        _settings.HoldHotkeyBindings = normalizedBindings;
        SyncLegacyHoldHotkeyFields();

        if (didChange)
        {
            SaveSettings();
        }
    }

    private HotkeyBindingSettings GetOrCreateHotkeyBinding(string action)
    {
        EnsureHotkeyBindingsInitialized();

        var binding = _settings.HotkeyBindings!.FirstOrDefault(existing =>
            string.Equals(existing.Action, action, StringComparison.OrdinalIgnoreCase));

        if (binding != null)
        {
            return binding;
        }

        binding = CreateDefaultBinding(action);
        _settings.HotkeyBindings!.Add(binding);
        SaveSettings();
        return binding;
    }

    private HoldHotkeyBindingSettings GetOrCreateHoldHotkeyBinding()
    {
        EnsureHoldHotkeyBindingsInitialized();

        var binding = _settings.HoldHotkeyBindings!.FirstOrDefault();
        if (binding != null)
        {
            return binding;
        }

        binding = CreateEmptyHoldBinding();
        _settings.HoldHotkeyBindings!.Add(binding);
        SyncLegacyHoldHotkeyFields();
        SaveSettings();
        return binding;
    }

    private HotkeyBindingSettings GetHotkeyBindingById(string bindingId)
    {
        var binding = _settings.HotkeyBindings!.FirstOrDefault(existing =>
            string.Equals(existing.Id, bindingId, StringComparison.OrdinalIgnoreCase));

        if (binding != null)
        {
            return binding;
        }

        throw new InvalidOperationException($"Hotkey binding '{bindingId}' was not found.");
    }

    private HoldHotkeyBindingSettings GetHoldHotkeyBindingById(string bindingId)
    {
        var binding = _settings.HoldHotkeyBindings!.FirstOrDefault(existing =>
            string.Equals(existing.Id, bindingId, StringComparison.OrdinalIgnoreCase));

        if (binding != null)
        {
            return binding;
        }

        throw new InvalidOperationException($"Hold hotkey binding '{bindingId}' was not found.");
    }

    private HotkeyBindingSettings CreateDefaultBinding(string action)
    {
        return action switch
        {
            HotkeyActions.Toggle => new HotkeyBindingSettings
            {
                Id = CreateBindingId(),
                Action = HotkeyActions.Toggle,
                KeyCode = _settings.HotkeyCode,
                Modifiers = _settings.HotkeyModifiers,
                IgnoreModifiers = _settings.IgnoreModifiers
            },
            HotkeyActions.Mute => new HotkeyBindingSettings
            {
                Id = CreateBindingId(),
                Action = HotkeyActions.Mute,
                KeyCode = 0,
                Modifiers = ModifierKeys.None,
                IgnoreModifiers = false
            },
            HotkeyActions.Unmute => new HotkeyBindingSettings
            {
                Id = CreateBindingId(),
                Action = HotkeyActions.Unmute,
                KeyCode = 0,
                Modifiers = ModifierKeys.None,
                IgnoreModifiers = false
            },
            _ => new HotkeyBindingSettings
            {
                Id = CreateBindingId(),
                Action = action,
                KeyCode = 0,
                Modifiers = ModifierKeys.None,
                IgnoreModifiers = false
            }
        };
    }

    private HotkeyBindingSettings CreateEmptyBinding(string action)
    {
        return new HotkeyBindingSettings
        {
            Id = CreateBindingId(),
            Action = action,
            KeyCode = 0,
            Modifiers = ModifierKeys.None,
            IgnoreModifiers = false
        };
    }

    private HoldHotkeyBindingSettings CreateEmptyHoldBinding()
    {
        return new HoldHotkeyBindingSettings
        {
            Id = CreateBindingId(),
            KeyCode = 0,
            Modifiers = ModifierKeys.None
        };
    }

    private void SyncLegacyHotkeyFields()
    {
        var toggleBinding = _settings.HotkeyBindings?
            .Where(binding => string.Equals(binding.Action, HotkeyActions.Toggle, StringComparison.OrdinalIgnoreCase))
            .OrderByDescending(binding => binding.KeyCode > 0 || binding.Modifiers != ModifierKeys.None)
            .FirstOrDefault();

        _settings.HotkeyCode = toggleBinding?.KeyCode ?? 0;
        _settings.HotkeyModifiers = toggleBinding?.Modifiers ?? ModifierKeys.None;
        _settings.IgnoreModifiers = toggleBinding?.IgnoreModifiers ?? false;
    }

    private void SyncLegacyHoldHotkeyFields()
    {
        var holdBinding = _settings.HoldHotkeyBindings?.FirstOrDefault();
        _settings.HoldHotkeyCode = holdBinding?.KeyCode ?? 0;
        _settings.HoldHotkeyModifiers = holdBinding?.Modifiers ?? ModifierKeys.None;
    }

    private static string CreateBindingId()
    {
        return Guid.NewGuid().ToString("N");
    }
}

public class AppSettings
{
    // Toggle hotkey settings
    public int HotkeyCode { get; set; } = 0; // Empty by default
    public ModifierKeys HotkeyModifiers { get; set; } = ModifierKeys.None;
    public bool IgnoreModifiers { get; set; } = false;
    public List<HotkeyBindingSettings>? HotkeyBindings { get; set; }
    
    // Hold hotkey settings
    public int HoldHotkeyCode { get; set; } = 0; // Disabled by default
    public ModifierKeys HoldHotkeyModifiers { get; set; } = ModifierKeys.None;
    public bool IgnoreHoldModifiers { get; set; } = false;
    public List<HoldHotkeyBindingSettings>? HoldHotkeyBindings { get; set; }
    public string HoldAction { get; set; } = "Toggle"; // Toggle, HoldToMute, HoldToUnmute
    public bool HoldPlaySounds { get; set; } = true; // Play sounds when using hold hotkey
    public bool HoldShowOverlay { get; set; } = true; // Show overlay when using hold hotkey
    
    // Hold hotkey sound settings (null = use default from Sounds tab)
    public string? HoldMuteSoundPreloaded { get; set; } = null; // null = use default
    public string? HoldMuteSoundCustomPath { get; set; } = null; // null = use default
    public string? HoldUnmuteSoundPreloaded { get; set; } = null; // null = use default
    public string? HoldUnmuteSoundCustomPath { get; set; } = null; // null = use default
    public float? HoldSoundVolume { get; set; } = null; // null = use default volume
    
    public string? SelectedMicrophoneId { get; set; }
    public bool AutoStartEnabled { get; set; } = false;
    public bool StartMinimized { get; set; } = false; // Show settings window on first launch
    public bool AutoMuteOnStartup { get; set; } = false;
    public bool AutoMuteAfterInactivityEnabled { get; set; } = false;
    public int AutoMuteAfterInactivityMinutes { get; set; } = 5;
    public bool AutoUnmuteOnActivity { get; set; } = false;
    public bool AutoMutePlaySounds { get; set; } = true;
    public bool CheckForUpdatesOnStartup { get; set; } = true; // Check for updates when app starts
    public DateTime? LastUpdateCheck { get; set; } // Last time we checked for updates
    public string TrayIconStyle { get; set; } = "Standard"; // Standard, FilledCircle, Dot
    public string? LanguageOverride { get; set; }
    
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
    public int OverlayScale { get; set; } = 100; // Overlay size scale (10-400%)
    public string OverlayVariant { get; set; } = "MicIcon"; // MicIcon, Dot
    public bool OverlayButtonMode { get; set; } = false; // Make overlay clickable and draggable
}

public static class HotkeyActions
{
    public const string Toggle = "Toggle";
    public const string Mute = "Mute";
    public const string Unmute = "Unmute";

    public static readonly string[] StandardActions = new[] { Toggle, Mute, Unmute };
}

public class HotkeyBindingSettings
{
    public string Id { get; set; } = "";
    public string Action { get; set; } = HotkeyActions.Toggle;
    public int KeyCode { get; set; }
    public ModifierKeys Modifiers { get; set; } = ModifierKeys.None;
    public bool IgnoreModifiers { get; set; }

    public HotkeyBindingSettings Clone()
    {
        return new HotkeyBindingSettings
        {
            Id = Id,
            Action = Action,
            KeyCode = KeyCode,
            Modifiers = Modifiers,
            IgnoreModifiers = IgnoreModifiers
        };
    }
}

public class HoldHotkeyBindingSettings
{
    public string Id { get; set; } = "";
    public int KeyCode { get; set; }
    public ModifierKeys Modifiers { get; set; } = ModifierKeys.None;

    public HoldHotkeyBindingSettings Clone()
    {
        return new HoldHotkeyBindingSettings
        {
            Id = Id,
            KeyCode = KeyCode,
            Modifiers = Modifiers
        };
    }
}
