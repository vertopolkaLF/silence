using Microsoft.UI.Xaml;
using silence_.Services;
using System;
using System.Linq;

namespace silence_
{
    public partial class App : Application
    {
        private MainWindow? _window;
        private MicrophoneService? _microphoneService;
        private KeyboardHookService? _keyboardHookService;
        private SettingsService? _settingsService;
        private bool _startMinimized;

        public static App? Instance { get; private set; }
        public MicrophoneService MicrophoneService => _microphoneService!;
        public KeyboardHookService KeyboardHookService => _keyboardHookService!;
        public SettingsService SettingsService => _settingsService!;
        public MainWindow? MainWindowInstance => _window;

        // Event for mute state changes
        public event Action<bool>? MuteStateChanged;

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
        }

        private void OnHotkeyPressed()
        {
            var isMuted = _microphoneService?.ToggleMute() ?? false;
            MuteStateChanged?.Invoke(isMuted);
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
            _window?.DisposeTrayIcon();
            _window?.Close();
            Environment.Exit(0);
        }
    }
}
