using Microsoft.UI.Xaml;
using Microsoft.UI.Dispatching;
using silence_.Services;
using System;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading.Tasks;

namespace silence_
{
    public partial class App : Application
    {
        private MainWindow? _window;
        private LayeredOverlay? _overlayWindow;
        private MicrophoneService? _microphoneService;
        private KeyboardHookService? _keyboardHookService;
        private GamepadInputService? _gamepadInputService;
        private SettingsService? _settingsService;
        private LocalizationService? _localizationService;
        private UpdateService? _updateService;
        private NotificationService? _notificationService;
        private SoundService? _soundService;
        private bool _startMinimized;
        private bool _isOverlayPositioning = false;
        private DispatcherQueueTimer? _previewTimer;
        private DispatcherQueueTimer? _positioningTimer;
        private DispatcherQueueTimer? _autoMuteTimer;
        private uint _lastAutoMuteInputTick;
        private bool _isMutedByInactivity;
        private POINT _autoMuteCursorPosition;
        private const int AutoMutePollingIntervalMs = 1000;
        private const int AutoUnmutePollingIntervalMs = 150;

        public static App? Instance { get; private set; }
        public MicrophoneService MicrophoneService => _microphoneService!;
        public KeyboardHookService KeyboardHookService => _keyboardHookService!;
        public GamepadInputService GamepadInputService => _gamepadInputService!;
        public SettingsService SettingsService => _settingsService!;
        public LocalizationService LocalizationService => _localizationService!;
        public UpdateService UpdateService => _updateService ??= new UpdateService();
        public NotificationService NotificationService => _notificationService ??= new NotificationService();
        public SoundService SoundService => _soundService ??= new SoundService();
        public MainWindow? MainWindowInstance => _window;

        // Event for mute state changes
        public event Action<bool>? MuteStateChanged;

        // Event for update available notification
        public event Action<UpdateCheckResult>? UpdateAvailable;

        // Event for overlay positioning stopped
        public event Action? OverlayPositioningStopped;

        // Cached update check result for AboutPage
        public UpdateCheckResult? LastUpdateCheckResult { get; private set; }

        public App()
        {
            Instance = this;
            InitializeComponent();

            var args = Environment.GetCommandLineArgs();
            _startMinimized = args.Contains("--minimized");

            // Initialize services
            _settingsService = new SettingsService();
            _settingsService.EnsureLanguageInitialized();
            _microphoneService = new MicrophoneService();
            _microphoneService.MuteStateChanged += OnMicrophoneServiceMuteStateChanged;
            _keyboardHookService = new KeyboardHookService();
            _gamepadInputService = new GamepadInputService(
                () => _keyboardHookService?.RecordingChordKeys ?? Array.Empty<int>(),
                () => _keyboardHookService?.PressedChordKeys ?? Array.Empty<int>());

            // Apply saved microphone selection
            if (!string.IsNullOrEmpty(_settingsService.Settings.SelectedMicrophoneId))
            {
                _microphoneService.SelectMicrophone(_settingsService.Settings.SelectedMicrophoneId);
            }

            // Setup hotkey with modifiers
            _keyboardHookService.HotkeyPressed += OnHotkeyPressed;
            _keyboardHookService.HoldHotkeyPressed += OnHoldHotkeyPressed;
            _keyboardHookService.HoldHotkeyReleased += OnHoldHotkeyReleased;
            _keyboardHookService.StartHook(
                _settingsService.GetHotkeyBindings(),
                _settingsService.GetHoldHotkeyBindings(),
                _settingsService.Settings.IgnoreHoldModifiers);

            _gamepadInputService.HotkeyPressed += OnHotkeyPressed;
            _gamepadInputService.HoldHotkeyPressed += OnHoldHotkeyPressed;
            _gamepadInputService.HoldHotkeyReleased += OnHoldHotkeyReleased;
            _gamepadInputService.StartMonitoring(
                _settingsService.GetHotkeyBindings(),
                _settingsService.GetHoldHotkeyBindings());
        }

        private void OnMicrophoneServiceMuteStateChanged(bool isMuted)
        {
            RunOnUiThread(() => OnMicrophoneServiceMuteStateChangedCore(isMuted));
        }

        private void OnMicrophoneServiceMuteStateChangedCore(bool isMuted)
        {
            if (!isMuted && _isMutedByInactivity)
            {
                ClearInactivityAutoMuteFlag();
            }

            var settings = _settingsService?.Settings;
            if (settings?.OverlayEnabled == true)
            {
                if (settings.OverlayVisibilityMode == "AfterToggle")
                {
                    if (_isOverlayPositioning)
                    {
                        _overlayWindow?.UpdateMuteState(isMuted);
                    }
                }
                else
                {
                    UpdateOverlayAfterStateChangeCore(isMuted);
                }
            }

            MuteStateChanged?.Invoke(isMuted);
        }

        protected override void OnLaunched(LaunchActivatedEventArgs args)
        {
            _localizationService ??= new LocalizationService(_settingsService!.Settings.LanguageOverride);
            _window = new MainWindow();

            // Initialize overlay window
            InitializeOverlay();
            RefreshAutoMuteMonitoring();
            ApplyStartupAutoMute();

            var shouldStartMinimized = _startMinimized || _settingsService!.Settings.StartMinimized;

            // Only activate window if NOT starting minimized
            // Tray icon is set up in MainWindow constructor, so it works without activation
            if (!shouldStartMinimized)
            {
                _window.Activate();
            }

            // Check for updates on startup if enabled
            if (_settingsService!.Settings.CheckForUpdatesOnStartup)
            {
                _ = CheckForUpdatesOnStartupAsync();
            }
        }

        private void InitializeOverlay()
        {
            _overlayWindow = new LayeredOverlay();

            // Set initial position and apply settings
            var settings = _settingsService?.Settings;
            if (settings != null)
            {
                _overlayWindow.ApplySettings();
                _overlayWindow.MoveToPosition(
                    settings.OverlayPositionX,
                    settings.OverlayPositionY,
                    settings.OverlayScreenId);
            }

            UpdateOverlayInputTimer();

            // Update overlay visibility based on current state
            UpdateOverlayVisibility();
        }

        public void RefreshOverlay()
        {
            RunOnUiThread(RefreshOverlayCore);
        }

        private void RefreshOverlayCore()
        {
            var wasPositioning = _isOverlayPositioning;

            _previewTimer?.Stop();
            _previewTimer = null;

            _positioningTimer?.Stop();
            _positioningTimer = null;

            _overlayWindow?.Dispose();
            _overlayWindow = null;

            InitializeOverlay();

            if (wasPositioning)
            {
                _overlayWindow?.StartPositioning();
                UpdateOverlayInputTimer();
            }
        }

        public void UpdateOverlayVisibility()
        {
            RunOnUiThread(UpdateOverlayVisibilityCore);
        }

        private void UpdateOverlayVisibilityCore()
        {
            if (_overlayWindow == null || _settingsService == null) return;

            var settings = _settingsService.Settings;
            UpdateOverlayInputTimer();

            if (!settings.OverlayEnabled)
            {
                _overlayWindow.HideOverlay();
                return;
            }

            var isMuted = _microphoneService?.IsMuted() ?? false;
            bool shouldShow = settings.OverlayVisibilityMode switch
            {
                "Always" => true,
                "WhenMuted" => isMuted,
                "WhenUnmuted" => !isMuted,
                "AfterToggle" => false, // Handled separately in OnHotkeyPressed
                _ => isMuted
            };

            if (shouldShow || _isOverlayPositioning)
            {
                // Only update mute state when showing - prevents animation to wrong state
                _overlayWindow.UpdateMuteState(isMuted);
                _overlayWindow.ShowOverlay();
            }
            else
            {
                _overlayWindow.HideOverlay();
            }
        }

        private void ShowOverlayTemporarily()
        {
            if (_overlayWindow == null || _settingsService == null) return;

            var settings = _settingsService.Settings;
            var isMuted = _microphoneService?.IsMuted() ?? false;

            _overlayWindow.UpdateMuteState(isMuted);
            _overlayWindow.ShowOverlay();

            // Cancel any existing timer
            _previewTimer?.Stop();

            // Set up timer to hide overlay after duration
            var durationMs = (int)(settings.OverlayShowDuration * 1000);
            var dispatcher = GetUiDispatcherQueue();
            if (dispatcher == null)
            {
                return;
            }

            _previewTimer = dispatcher.CreateTimer();
            _previewTimer.Interval = TimeSpan.FromMilliseconds(durationMs);
            _previewTimer.IsRepeating = false;
            _previewTimer.Tick += (_, _) =>
            {
                _previewTimer?.Stop();

                // Only hide if still in AfterToggle mode and not positioning
                if (_settingsService?.Settings.OverlayVisibilityMode == "AfterToggle" && !_isOverlayPositioning)
                {
                    _overlayWindow?.HideOverlay();
                }
            };
            _previewTimer.Start();
        }

        public void UpdateOverlayPosition()
        {
            RunOnUiThread(UpdateOverlayPositionCore);
        }

        private void UpdateOverlayPositionCore()
        {
            if (_overlayWindow == null || _settingsService == null) return;

            var settings = _settingsService.Settings;
            _overlayWindow.MoveToPosition(
                settings.OverlayPositionX,
                settings.OverlayPositionY,
                settings.OverlayScreenId);
        }

        public void OnDisplayChanged()
        {
            RunOnUiThread(OnDisplayChangedCore);
        }

        private void OnDisplayChangedCore()
        {
            // Called when display resolution or DPI changes
            _overlayWindow?.OnDisplayChanged();
        }

        public void ApplyOverlaySettings()
        {
            RunOnUiThread(ApplyOverlaySettingsCore);
        }

        private void ApplyOverlaySettingsCore()
        {
            if (_overlayWindow == null || _settingsService == null) return;

            _overlayWindow.ApplySettings();
            _overlayWindow.SetButtonMode(_settingsService.Settings.OverlayButtonMode);
            UpdateOverlayPosition();
            UpdateOverlayInputTimer();
        }

        public void StartOverlayPositioning()
        {
            RunOnUiThread(StartOverlayPositioningCore);
        }

        private void StartOverlayPositioningCore()
        {
            if (_overlayWindow == null) return;

            _isOverlayPositioning = true;
            _overlayWindow.StartPositioning();

            UpdateOverlayInputTimer();
        }

        public void StopOverlayPositioning()
        {
            RunOnUiThread(StopOverlayPositioningCore);
        }

        private void StopOverlayPositioningCore()
        {
            if (_overlayWindow == null) return;

            _isOverlayPositioning = false;
            _overlayWindow.StopPositioning();
            UpdateOverlayInputTimer();
            UpdateOverlayVisibility();

            // Notify UI to reset button state
            OverlayPositioningStopped?.Invoke();
        }

        public void PreviewOverlay()
        {
            RunOnUiThread(PreviewOverlayCore);
        }

        private void PreviewOverlayCore()
        {
            if (_overlayWindow == null) return;

            // Show overlay for 3 seconds
            _overlayWindow.UpdateMuteState(_microphoneService?.IsMuted() ?? false);
            _overlayWindow.ShowOverlay();

            // Use timer to hide after preview
            _previewTimer?.Stop();
            var dispatcher = GetUiDispatcherQueue();
            if (dispatcher == null)
            {
                return;
            }

            _previewTimer = dispatcher.CreateTimer();
            _previewTimer.Interval = TimeSpan.FromMilliseconds(3000);
            _previewTimer.IsRepeating = false;
            _previewTimer.Tick += (_, _) =>
            {
                _previewTimer?.Stop();
                UpdateOverlayVisibilityCore();
            };
            _previewTimer.Start();
        }

        private void UpdateOverlayInputTimer()
        {
            if (_overlayWindow == null || _settingsService == null) return;

            bool shouldProcessInput = _isOverlayPositioning ||
                (_settingsService.Settings.OverlayEnabled && _settingsService.Settings.OverlayButtonMode);

            if (!shouldProcessInput)
            {
                _positioningTimer?.Stop();
                _positioningTimer = null;
                return;
            }

            if (_positioningTimer != null)
            {
                return;
            }

            var dispatcher = GetUiDispatcherQueue();
            if (dispatcher == null)
            {
                return;
            }

            _positioningTimer = dispatcher.CreateTimer();
            _positioningTimer.Interval = TimeSpan.FromMilliseconds(16); // ~60fps
            _positioningTimer.IsRepeating = true;
            _positioningTimer.Tick += (_, _) => _overlayWindow?.ProcessDrag();
            _positioningTimer.Start();
        }

        public void RefreshAutoMuteMonitoring()
        {
            RunOnUiThread(RefreshAutoMuteMonitoringCore);
        }

        private void RefreshAutoMuteMonitoringCore()
        {
            var settings = _settingsService?.Settings;
            if (settings == null)
            {
                return;
            }

            var shouldMonitor = settings.AutoMuteAfterInactivityEnabled &&
                settings.AutoMuteAfterInactivityMinutes > 0;

            if (!shouldMonitor)
            {
                _autoMuteTimer?.Stop();
                _autoMuteTimer = null;
                _lastAutoMuteInputTick = 0;
                ClearInactivityAutoMuteFlag();
                return;
            }

            if (_autoMuteTimer != null)
            {
                UpdateAutoMuteTimerInterval();
                EvaluateAutoMuteInactivity();
                return;
            }

            var dispatcher = GetUiDispatcherQueue();
            if (dispatcher == null)
            {
                return;
            }

            _autoMuteTimer = dispatcher.CreateTimer();
            UpdateAutoMuteTimerInterval();
            _autoMuteTimer.IsRepeating = true;
            _autoMuteTimer.Tick += (_, _) => EvaluateAutoMuteInactivity();
            _autoMuteTimer.Start();

            EvaluateAutoMuteInactivity();
        }

        private void ApplyStartupAutoMute()
        {
            var settings = _settingsService?.Settings;
            if (settings?.AutoMuteOnStartup != true)
            {
                return;
            }

            ApplyAutoMute(playSound: settings.AutoMutePlaySounds, fromInactivity: false);
        }

        private void EvaluateAutoMuteInactivity()
        {
            var settings = _settingsService?.Settings;
            if (settings == null ||
                !settings.AutoMuteAfterInactivityEnabled ||
                settings.AutoMuteAfterInactivityMinutes <= 0)
            {
                return;
            }

            if (_isMutedByInactivity && (_microphoneService?.IsMuted() ?? false) == false)
            {
                ClearInactivityAutoMuteFlag();
            }

            if (_isMutedByInactivity && settings.AutoUnmuteOnActivity && TryAutoUnmuteFromMouseMovement())
            {
                return;
            }

            var lastInputTick = GetLastInputTick();
            if (lastInputTick == 0)
            {
                return;
            }

            var idleTime = GetIdleTime(lastInputTick);
            var threshold = TimeSpan.FromMinutes(settings.AutoMuteAfterInactivityMinutes);
            if (idleTime < threshold)
            {
                return;
            }

            if (_lastAutoMuteInputTick == lastInputTick)
            {
                return;
            }

            _lastAutoMuteInputTick = lastInputTick;
            ApplyAutoMute(playSound: settings.AutoMutePlaySounds, fromInactivity: true);
        }

        private void ApplyAutoMute(bool playSound, bool fromInactivity)
        {
            var settings = _settingsService?.Settings;
            if (settings == null)
            {
                return;
            }

            var isMuted = _microphoneService?.IsMuted() ?? false;
            if (isMuted)
            {
                return;
            }

            var newState = _microphoneService?.SetMute(true);
            if (newState != true)
            {
                return;
            }

            if (fromInactivity)
            {
                _isMutedByInactivity = true;
                _autoMuteCursorPosition = GetCursorPositionOrDefault();
                UpdateAutoMuteTimerInterval();
            }
            else
            {
                ClearInactivityAutoMuteFlag();
            }

            UpdateOverlayAfterStateChange(true);

            if (playSound)
            {
                PlayDefaultSoundFeedback(true);
            }
        }

        private bool TryAutoUnmuteFromMouseMovement()
        {
            var currentPosition = GetCursorPositionOrDefault();
            if (currentPosition.X == _autoMuteCursorPosition.X &&
                currentPosition.Y == _autoMuteCursorPosition.Y)
            {
                return false;
            }

            var newState = _microphoneService?.SetMute(false);
            if (newState != false)
            {
                return false;
            }

            ClearInactivityAutoMuteFlag();
            UpdateOverlayAfterStateChange(false);
            return true;
        }

        private void ClearInactivityAutoMuteFlag()
        {
            _isMutedByInactivity = false;
            UpdateAutoMuteTimerInterval();
        }

        private void UpdateAutoMuteTimerInterval()
        {
            if (_autoMuteTimer == null)
            {
                return;
            }

            var settings = _settingsService?.Settings;
            var intervalMs = _isMutedByInactivity && settings?.AutoUnmuteOnActivity == true
                ? AutoUnmutePollingIntervalMs
                : AutoMutePollingIntervalMs;

            _autoMuteTimer.Interval = TimeSpan.FromMilliseconds(intervalMs);
        }

        private static POINT GetCursorPositionOrDefault()
        {
            return GetCursorPos(out var point) ? point : default;
        }

        private static TimeSpan GetIdleTime(uint lastInputTick)
        {
            var currentTick = unchecked((uint)Environment.TickCount);
            var elapsedMilliseconds = currentTick - lastInputTick;
            return TimeSpan.FromMilliseconds(elapsedMilliseconds);
        }

        private static uint GetLastInputTick()
        {
            var info = new LASTINPUTINFO
            {
                cbSize = (uint)Marshal.SizeOf<LASTINPUTINFO>()
            };

            return GetLastInputInfo(ref info) ? info.dwTime : 0;
        }

        private async Task CheckForUpdatesOnStartupAsync()
        {
            try
            {
                // Small delay to let the app fully initialize
                await Task.Delay(2000);

                System.Diagnostics.Debug.WriteLine("CheckForUpdatesOnStartupAsync: Starting update check");

                var result = await UpdateService.CheckForUpdatesAsync();

                System.Diagnostics.Debug.WriteLine($"CheckForUpdatesOnStartupAsync: Update available = {result.IsUpdateAvailable}");

                if (result.Success && result.IsUpdateAvailable)
                {
                    LastUpdateCheckResult = result;
                    UpdateAvailable?.Invoke(result);

                    System.Diagnostics.Debug.WriteLine("CheckForUpdatesOnStartupAsync: Calling SendUpdateNotification");
                    // Send toast notification
                    NotificationService.SendUpdateNotification(result);
                }

                _settingsService?.UpdateLastUpdateCheck();
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"CheckForUpdatesOnStartupAsync: Exception - {ex.Message}");
                // Silent fail on startup check - don't bother user
            }
        }

        private bool ExecuteDirectHotkeyAction(string action, out bool stateChanged)
        {
            ClearInactivityAutoMuteFlag();

            var wasMuted = _microphoneService?.IsMuted() ?? false;
            var isMuted = action switch
            {
                HotkeyActions.Mute => _microphoneService?.SetMute(true) ?? wasMuted,
                HotkeyActions.Unmute => _microphoneService?.SetMute(false) ?? wasMuted,
                _ => _microphoneService?.ToggleMute() ?? false
            };

            stateChanged = wasMuted != isMuted;

            return isMuted;
        }

        private void UpdateOverlayAfterStateChange(bool isMuted)
        {
            RunOnUiThread(() => UpdateOverlayAfterStateChangeCore(isMuted));
        }

        private void UpdateOverlayAfterStateChangeCore(bool isMuted)
        {
            var settings = _settingsService?.Settings;
            if (settings?.OverlayEnabled == true && settings.OverlayVisibilityMode == "AfterToggle")
            {
                ShowOverlayTemporarily();
                return;
            }

            bool willBeVisible = settings?.OverlayEnabled == true && settings.OverlayVisibilityMode switch
            {
                "Always" => true,
                "WhenMuted" => isMuted,
                "WhenUnmuted" => !isMuted,
                _ => isMuted
            };

            if (willBeVisible)
            {
                _overlayWindow?.UpdateMuteState(isMuted);
            }

            UpdateOverlayVisibilityCore();
        }

        private void PlayDefaultSoundFeedback(bool isMuted)
        {
            var settings = _settingsService?.Settings;
            if (settings == null)
            {
                return;
            }

            if (isMuted)
            {
                SoundService.PlayMuteSound(settings);
            }
            else
            {
                SoundService.PlayUnmuteSound(settings);
            }
        }

        private void PlayHoldSoundFeedback(bool isMuted, AppSettings settings)
        {
            var mutePreloaded = settings.HoldMuteSoundPreloaded ?? settings.MuteSoundPreloaded;
            var muteCustom = settings.HoldMuteSoundCustomPath ?? settings.MuteSoundCustomPath;
            var unmutePreloaded = settings.HoldUnmuteSoundPreloaded ?? settings.UnmuteSoundPreloaded;
            var unmuteCustom = settings.HoldUnmuteSoundCustomPath ?? settings.UnmuteSoundCustomPath;
            var volume = settings.HoldSoundVolume ?? settings.SoundVolume;

            if (isMuted)
            {
                var path = SoundService.GetSoundPath(mutePreloaded, muteCustom, true);
                SoundService.PlaySound(path, volume);
            }
            else
            {
                var path = SoundService.GetSoundPath(unmutePreloaded, unmuteCustom, false);
                SoundService.PlaySound(path, volume);
            }
        }

        private void OnHotkeyPressed(string action)
        {
            var isMuted = ExecuteDirectHotkeyAction(action, out var stateChanged);
            if (!stateChanged)
            {
                return;
            }

            UpdateOverlayAfterStateChange(isMuted);
            PlayDefaultSoundFeedback(isMuted);
        }

        private void OnHoldHotkeyPressed()
        {
            ClearInactivityAutoMuteFlag();

            var settings = _settingsService?.Settings;
            var action = settings?.HoldAction ?? "Toggle";
            var wasMuted = _microphoneService?.IsMuted() ?? false;
            bool isMuted;

            switch (action)
            {
                case "HoldToMute":
                    // Force mute while holding
                    if (!wasMuted)
                    {
                        isMuted = _microphoneService?.SetMute(true) ?? wasMuted;
                    }
                    else
                    {
                        isMuted = wasMuted;
                    }
                    break;

                case "HoldToUnmute":
                    // Force unmute while holding
                    if (wasMuted)
                    {
                        isMuted = _microphoneService?.SetMute(false) ?? wasMuted;
                    }
                    else
                    {
                        isMuted = wasMuted;
                    }
                    break;

                case "Toggle":
                default:
                    // Toggle current state
                    isMuted = _microphoneService?.ToggleMute() ?? false;
                    break;
            }

            // Update overlay if enabled for hold hotkey
            if (settings?.HoldShowOverlay == true)
            {
                UpdateOverlayAfterStateChange(isMuted);
            }

            // Play sound feedback if enabled for hold hotkey
            if (settings?.HoldPlaySounds == true && settings != null)
            {
                PlayHoldSoundFeedback(isMuted, settings);
            }
        }

        private void OnHoldHotkeyReleased()
        {
            ClearInactivityAutoMuteFlag();

            var settings = _settingsService?.Settings;
            var action = settings?.HoldAction ?? "Toggle";
            var wasMuted = _microphoneService?.IsMuted() ?? false;
            bool isMuted;

            switch (action)
            {
                case "HoldToMute":
                    // Unmute on release (if we muted on press)
                    if (wasMuted)
                    {
                        isMuted = _microphoneService?.SetMute(false) ?? wasMuted;
                    }
                    else
                    {
                        isMuted = wasMuted;
                    }
                    break;

                case "HoldToUnmute":
                    // Mute on release (if we unmuted on press)
                    if (!wasMuted)
                    {
                        isMuted = _microphoneService?.SetMute(true) ?? wasMuted;
                    }
                    else
                    {
                        isMuted = wasMuted;
                    }
                    break;

                case "Toggle":
                default:
                    // Toggle back to original state
                    isMuted = _microphoneService?.ToggleMute() ?? false;
                    break;
            }

            // Update overlay if enabled for hold hotkey
            if (settings?.HoldShowOverlay == true)
            {
                UpdateOverlayAfterStateChange(isMuted);
            }

            // Play sound feedback if enabled for hold hotkey
            if (settings?.HoldPlaySounds == true && settings != null)
            {
                PlayHoldSoundFeedback(isMuted, settings);
            }
        }

        public void ToggleMute()
        {
            OnHotkeyPressed(HotkeyActions.Toggle);
        }

        public void ApplyLanguageOverride(string? languageOverride)
        {
            var resolvedLanguage = LocalizationService.ResolveAppLanguage(languageOverride);
            if (_settingsService?.Settings.LanguageOverride == resolvedLanguage)
            {
                return;
            }

            _settingsService?.UpdateLanguageOverride(resolvedLanguage);
            _localizationService?.ApplyLanguage(resolvedLanguage);
            _window?.RefreshLocalizedUI();
            RefreshOverlay();
        }

        public void HideMainWindow()
        {
            _window?.HideToTray();
        }

        public void ExitApplication()
        {
            _keyboardHookService?.Dispose();
            _gamepadInputService?.Dispose();
            _microphoneService?.Dispose();
            _updateService?.Dispose();
            _notificationService?.Dispose();
            _soundService?.Dispose();
            _previewTimer?.Stop();
            _positioningTimer?.Stop();
            _autoMuteTimer?.Stop();
            _overlayWindow?.Dispose();
            _overlayWindow = null;
            _window?.DisposeTrayIcon();
            _window?.Close();
            Environment.Exit(0);
        }

        private DispatcherQueue? GetUiDispatcherQueue()
        {
            return _window?.DispatcherQueue ?? DispatcherQueue.GetForCurrentThread();
        }

        private void RunOnUiThread(Action action)
        {
            var dispatcher = GetUiDispatcherQueue();
            if (dispatcher == null || dispatcher.HasThreadAccess)
            {
                action();
                return;
            }

            if (!dispatcher.TryEnqueue(() => action()))
            {
                System.Diagnostics.Debug.WriteLine("App: failed to marshal overlay action to UI thread.");
            }
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct LASTINPUTINFO
        {
            public uint cbSize;
            public uint dwTime;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct POINT
        {
            public int X;
            public int Y;
        }

        [DllImport("user32.dll")]
        private static extern bool GetLastInputInfo(ref LASTINPUTINFO plii);

        [DllImport("user32.dll")]
        private static extern bool GetCursorPos(out POINT lpPoint);
    }
}
