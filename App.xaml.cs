using Microsoft.UI.Xaml;
using silence_.Services;
using System;
using System.Linq;
using System.Threading.Tasks;

namespace silence_
{
    public partial class App : Application
    {
        private MainWindow? _window;
        private OverlayWindow? _overlayWindow;
        private MicrophoneService? _microphoneService;
        private KeyboardHookService? _keyboardHookService;
        private SettingsService? _settingsService;
        private UpdateService? _updateService;
        private SoundService? _soundService;
        private bool _startMinimized;
        private bool _isOverlayPositioning = false;
        private System.Timers.Timer? _previewTimer;

        public static App? Instance { get; private set; }
        public MicrophoneService MicrophoneService => _microphoneService!;
        public KeyboardHookService KeyboardHookService => _keyboardHookService!;
        public SettingsService SettingsService => _settingsService!;
        public UpdateService UpdateService => _updateService ??= new UpdateService();
        public SoundService SoundService => _soundService ??= new SoundService();
        public MainWindow? MainWindowInstance => _window;

        // Event for mute state changes
        public event Action<bool>? MuteStateChanged;
        
        // Event for update available notification
        public event Action<UpdateCheckResult>? UpdateAvailable;
        
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
            _microphoneService = new MicrophoneService();
            _keyboardHookService = new KeyboardHookService();

            // Apply saved microphone selection
            if (!string.IsNullOrEmpty(_settingsService.Settings.SelectedMicrophoneId))
            {
                _microphoneService.SelectMicrophone(_settingsService.Settings.SelectedMicrophoneId);
            }

            // Setup hotkey with modifiers
            _keyboardHookService.HotkeyPressed += OnHotkeyPressed;
            _keyboardHookService.StartHook(
                _settingsService.Settings.HotkeyCode,
                _settingsService.Settings.HotkeyModifiers,
                _settingsService.Settings.IgnoreModifiers);
        }

        protected override void OnLaunched(LaunchActivatedEventArgs args)
        {
            _window = new MainWindow();
            
            // Initialize overlay window
            InitializeOverlay();
            
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
            _overlayWindow = new OverlayWindow();
            
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
            
            // Update overlay visibility based on current state
            UpdateOverlayVisibility();
        }
        
        public void UpdateOverlayVisibility()
        {
            if (_overlayWindow == null || _settingsService == null) return;
            
            var settings = _settingsService.Settings;
            
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
            _previewTimer?.Dispose();
            
            // Set up timer to hide overlay after duration
            var durationMs = (int)(settings.OverlayShowDuration * 1000);
            _previewTimer = new System.Timers.Timer(durationMs);
            _previewTimer.Elapsed += (s, e) =>
            {
                _previewTimer?.Stop();
                _window?.DispatcherQueue.TryEnqueue(() =>
                {
                    // Only hide if still in AfterToggle mode and not positioning
                    if (_settingsService?.Settings.OverlayVisibilityMode == "AfterToggle" && !_isOverlayPositioning)
                    {
                        _overlayWindow?.HideOverlay();
                    }
                });
            };
            _previewTimer.AutoReset = false;
            _previewTimer.Start();
        }
        
        public void UpdateOverlayPosition()
        {
            if (_overlayWindow == null || _settingsService == null) return;
            
            var settings = _settingsService.Settings;
            _overlayWindow.MoveToPosition(
                settings.OverlayPositionX,
                settings.OverlayPositionY,
                settings.OverlayScreenId);
        }
        
        public void ApplyOverlaySettings()
        {
            if (_overlayWindow == null) return;
            
            _overlayWindow.ApplySettings();
            UpdateOverlayPosition();
        }
        
        public void StartOverlayPositioning()
        {
            if (_overlayWindow == null) return;
            
            _isOverlayPositioning = true;
            _overlayWindow.StartPositioning();
        }
        
        public void StopOverlayPositioning()
        {
            if (_overlayWindow == null) return;
            
            _isOverlayPositioning = false;
            _overlayWindow.StopPositioning();
            UpdateOverlayVisibility();
        }
        
        public void PreviewOverlay()
        {
            if (_overlayWindow == null) return;
            
            // Show overlay for 3 seconds
            _overlayWindow.UpdateMuteState(_microphoneService?.IsMuted() ?? false);
            _overlayWindow.ShowOverlay();
            
            // Use timer to hide after preview
            _previewTimer?.Stop();
            _previewTimer?.Dispose();
            _previewTimer = new System.Timers.Timer(3000);
            _previewTimer.Elapsed += (s, e) =>
            {
                _previewTimer?.Stop();
                _window?.DispatcherQueue.TryEnqueue(() =>
                {
                    UpdateOverlayVisibility();
                });
            };
            _previewTimer.AutoReset = false;
            _previewTimer.Start();
        }
        
        private async Task CheckForUpdatesOnStartupAsync()
        {
            try
            {
                // Small delay to let the app fully initialize
                await Task.Delay(2000);
                
                var result = await UpdateService.CheckForUpdatesAsync();
                
                if (result.Success && result.IsUpdateAvailable)
                {
                    LastUpdateCheckResult = result;
                    UpdateAvailable?.Invoke(result);
                }
                
                _settingsService?.UpdateLastUpdateCheck();
            }
            catch
            {
                // Silent fail on startup check - don't bother user
            }
        }

        private void OnHotkeyPressed()
        {
            var isMuted = _microphoneService?.ToggleMute() ?? false;
            MuteStateChanged?.Invoke(isMuted);
            
            // Update overlay based on visibility mode
            var settings = _settingsService?.Settings;
            if (settings?.OverlayEnabled == true && settings.OverlayVisibilityMode == "AfterToggle")
            {
                // Show overlay temporarily for AfterToggle mode
                ShowOverlayTemporarily();
            }
            else
            {
                _overlayWindow?.UpdateMuteState(isMuted);
                UpdateOverlayVisibility();
            }
            
            // Play sound feedback
            if (settings != null)
            {
                if (isMuted)
                {
                    SoundService.PlayMuteSound(settings);
                }
                else
                {
                    SoundService.PlayUnmuteSound(settings);
                }
            }
        }

        public void ToggleMute()
        {
            OnHotkeyPressed();
        }

        public void HideMainWindow()
        {
            _window?.HideToTray();
        }

        public void ExitApplication()
        {
            _keyboardHookService?.Dispose();
            _microphoneService?.Dispose();
            _updateService?.Dispose();
            _soundService?.Dispose();
            _previewTimer?.Stop();
            _previewTimer?.Dispose();
            _overlayWindow?.Close();
            _window?.DisposeTrayIcon();
            _window?.Close();
            Environment.Exit(0);
        }
    }
}
