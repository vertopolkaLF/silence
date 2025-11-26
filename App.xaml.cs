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
        private MicrophoneService? _microphoneService;
        private KeyboardHookService? _keyboardHookService;
        private SettingsService? _settingsService;
        private UpdateService? _updateService;
        private SoundService? _soundService;
        private bool _startMinimized;

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
            
            // Play sound feedback
            var settings = _settingsService?.Settings;
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
            _window?.DisposeTrayIcon();
            _window?.Close();
            Environment.Exit(0);
        }
    }
}
