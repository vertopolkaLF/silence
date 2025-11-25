using H.NotifyIcon;
using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Silence_.Services;
using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Runtime.InteropServices;
using Windows.Graphics;
using WinRT.Interop;

namespace Silence_
{
    public sealed partial class MainWindow : Window
    {
        private TaskbarIcon? _trayIcon;
        private AppWindow? _appWindow;
        private bool _isRecordingHotkey;
        private bool _isMuted;
        private int _recordedKeyCode;
        private ModifierKeys _recordedModifiers;

        public bool StartMinimized { get; set; }

        public MainWindow()
        {
            InitializeComponent();
            
            SetupWindow();
            SetupTrayIcon();
            LoadSettings();
            
            // Subscribe to hotkey recording events
            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.KeyPressed += OnKeyPressed;
                App.Instance.KeyboardHookService.ModifiersChanged += OnModifiersChanged;
            }

            this.Closed += MainWindow_Closed;
            UpdateMuteState(App.Instance?.MicrophoneService.IsMuted() ?? false);
            this.Activated += MainWindow_Activated;
        }

        private bool _firstActivation = true;
        private void MainWindow_Activated(object sender, WindowActivatedEventArgs args)
        {
            if (_firstActivation && StartMinimized)
            {
                _firstActivation = false;
                HideToTray();
            }
        }

        private void SetupWindow()
        {
            var hwnd = WindowNative.GetWindowHandle(this);
            var windowId = Win32Interop.GetWindowIdFromWindow(hwnd);
            _appWindow = AppWindow.GetFromWindowId(windowId);

            if (_appWindow != null)
            {
                _appWindow.Resize(new SizeInt32(450, 550));
                
                var displayArea = DisplayArea.GetFromWindowId(windowId, DisplayAreaFallback.Primary);
                if (displayArea != null)
                {
                    var centerX = (displayArea.WorkArea.Width - 450) / 2;
                    var centerY = (displayArea.WorkArea.Height - 550) / 2;
                    _appWindow.Move(new PointInt32(centerX, centerY));
                }

                _appWindow.Title = "Silence!";
            }
        }

        private void SetupTrayIcon()
        {
            _trayIcon = new TaskbarIcon();
            
            var menu = new MenuFlyout();
            
            var showItem = new MenuFlyoutItem { Text = "Show Settings" };
            showItem.Click += (s, e) => ShowWindow();
            menu.Items.Add(showItem);

            var muteItem = new MenuFlyoutItem { Text = "Toggle Mute" };
            muteItem.Click += (s, e) => App.Instance?.ToggleMute();
            menu.Items.Add(muteItem);

            menu.Items.Add(new MenuFlyoutSeparator());

            var exitItem = new MenuFlyoutItem { Text = "Exit" };
            exitItem.Click += (s, e) => App.Instance?.ExitApplication();
            menu.Items.Add(exitItem);

            _trayIcon.ContextFlyout = menu;
            _trayIcon.LeftClickCommand = new RelayCommand(() => App.Instance?.ToggleMute());
            _trayIcon.DoubleClickCommand = new RelayCommand(ShowWindow);
            _trayIcon.ToolTipText = "Silence! - Microphone ON";

            UpdateTrayIcon(false);
            _trayIcon.ForceCreate();
        }

        private void LoadSettings()
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null) return;

            RefreshMicrophones();

            // Load hotkey with modifiers
            HotkeyTextBox.Text = VirtualKeys.GetHotkeyDisplayString(settings.HotkeyCode, settings.HotkeyModifiers);
            IgnoreModifiersCheckBox.IsChecked = settings.IgnoreModifiers;

            AutoStartCheckBox.IsChecked = App.Instance?.SettingsService.IsAutoStartEnabled() ?? false;
            StartMinimizedCheckBox.IsChecked = settings.StartMinimized;
        }

        private void RefreshMicrophones()
        {
            var microphones = App.Instance?.MicrophoneService.GetMicrophones();
            if (microphones == null) return;

            MicrophoneComboBox.Items.Clear();
            
            var defaultItem = new ComboBoxItem { Content = "Default Microphone", Tag = (string?)null };
            MicrophoneComboBox.Items.Add(defaultItem);

            int selectedIndex = 0;
            var selectedId = App.Instance?.SettingsService.Settings.SelectedMicrophoneId;

            for (int i = 0; i < microphones.Count; i++)
            {
                var mic = microphones[i];
                var item = new ComboBoxItem 
                { 
                    Content = mic.IsDefault ? $"{mic.Name} (Default)" : mic.Name,
                    Tag = mic.Id 
                };
                MicrophoneComboBox.Items.Add(item);

                if (mic.Id == selectedId)
                {
                    selectedIndex = i + 1;
                }
            }

            MicrophoneComboBox.SelectedIndex = selectedIndex;
        }

        public void UpdateMuteState(bool isMuted)
        {
            _isMuted = isMuted;
            
            DispatcherQueue.TryEnqueue(() =>
            {
                MuteStatusText.Text = isMuted ? "Microphone MUTED" : "Microphone ON";
                
                // Button color changes, text stays white
                if (isMuted)
                {
                    MuteButton.Background = new SolidColorBrush(Microsoft.UI.Colors.IndianRed);
                }
                else
                {
                    MuteButton.Background = new SolidColorBrush(Microsoft.UI.Colors.ForestGreen);
                }

                UpdateTrayIcon(isMuted);
            });
        }

        private void UpdateTrayIcon(bool isMuted)
        {
            if (_trayIcon == null) return;

            try
            {
                var icon = CreateMicrophoneIcon(isMuted);
                _trayIcon.Icon = icon;
                _trayIcon.ToolTipText = isMuted ? "Silence! - Microphone MUTED" : "Silence! - Microphone ON";
            }
            catch
            {
                // Icon creation failed
            }
        }

        private static Icon CreateMicrophoneIcon(bool isMuted)
        {
            const int size = 32;
            using var bitmap = new Bitmap(size, size);
            using var g = Graphics.FromImage(bitmap);
            
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(System.Drawing.Color.Transparent);

            var micColor = isMuted 
                ? System.Drawing.Color.FromArgb(220, 53, 69)
                : System.Drawing.Color.FromArgb(40, 167, 69);
            
            using var brush = new SolidBrush(micColor);
            using var pen = new Pen(micColor, 2);

            g.FillEllipse(brush, 10, 4, 12, 8);
            g.FillRectangle(brush, 10, 8, 12, 12);
            g.FillEllipse(brush, 10, 12, 12, 8);
            g.DrawArc(pen, 6, 12, 20, 12, 0, 180);
            g.DrawLine(pen, 16, 24, 16, 28);
            g.DrawLine(pen, 10, 28, 22, 28);

            if (isMuted)
            {
                using var whitePen = new Pen(System.Drawing.Color.White, 4);
                g.DrawLine(whitePen, 4, 4, 28, 28);
                g.DrawLine(whitePen, 28, 4, 4, 28);
                
                using var redPen = new Pen(System.Drawing.Color.FromArgb(220, 53, 69), 2);
                g.DrawLine(redPen, 4, 4, 28, 28);
                g.DrawLine(redPen, 28, 4, 4, 28);
            }

            return Icon.FromHandle(bitmap.GetHicon());
        }

        private void MainWindow_Closed(object sender, WindowEventArgs args)
        {
            args.Handled = true;
            HideToTray();
        }

        private void HideToTray()
        {
            _appWindow?.Hide();
        }

        private void ShowWindow()
        {
            _appWindow?.Show();
            
            if (_appWindow != null)
            {
                var hwnd = WindowNative.GetWindowHandle(this);
                SetForegroundWindow(hwnd);
            }
        }

        [DllImport("user32.dll")]
        private static extern bool SetForegroundWindow(IntPtr hWnd);

        #region Event Handlers

        private void MuteButton_Click(object sender, RoutedEventArgs e)
        {
            App.Instance?.ToggleMute();
        }

        private void MicrophoneComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (MicrophoneComboBox.SelectedItem is ComboBoxItem item)
            {
                var deviceId = item.Tag as string;
                App.Instance?.MicrophoneService.SelectMicrophone(deviceId);
                App.Instance?.SettingsService.UpdateSelectedMicrophone(deviceId);
            }
        }

        private void RecordHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            if (_isRecordingHotkey)
            {
                StopRecordingHotkey();
            }
            else
            {
                StartRecordingHotkey();
            }
        }

        private void StartRecordingHotkey()
        {
            _isRecordingHotkey = true;
            _recordedKeyCode = 0;
            _recordedModifiers = ModifierKeys.None;
            RecordHotkeyButton.Content = "Cancel";
            HotkeyTextBox.Text = "Press keys...";
            
            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.ResetRecordingState();
                App.Instance.KeyboardHookService.IsRecording = true;
            }
        }

        private void StopRecordingHotkey()
        {
            _isRecordingHotkey = false;
            RecordHotkeyButton.Content = "Record";
            
            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.IsRecording = false;
                App.Instance.KeyboardHookService.ResetRecordingState();
            }
        }

        private void OnModifiersChanged(ModifierKeys modifiers)
        {
            if (!_isRecordingHotkey) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                _recordedModifiers = modifiers;
                // Show current modifiers in real-time
                var display = VirtualKeys.GetHotkeyDisplayString(0, modifiers);
                HotkeyTextBox.Text = string.IsNullOrEmpty(display) ? "Press keys..." : display + " + ...";
            });
        }

        private void OnKeyPressed(int keyCode, ModifierKeys modifiers)
        {
            if (!_isRecordingHotkey) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                _recordedKeyCode = keyCode;
                _recordedModifiers = modifiers;

                // Update UI
                HotkeyTextBox.Text = VirtualKeys.GetHotkeyDisplayString(keyCode, modifiers);
                StopRecordingHotkey();

                // Save and apply new hotkey
                App.Instance?.SettingsService.UpdateHotkey(keyCode, modifiers);
                App.Instance?.KeyboardHookService.UpdateHotkey(
                    keyCode,
                    modifiers,
                    IgnoreModifiersCheckBox.IsChecked ?? true);
            });
        }

        private void IgnoreModifiersCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            var ignore = IgnoreModifiersCheckBox.IsChecked ?? true;
            App.Instance?.SettingsService.UpdateIgnoreModifiers(ignore);
            
            var settings = App.Instance?.SettingsService.Settings;
            if (settings != null)
            {
                App.Instance?.KeyboardHookService.UpdateHotkey(
                    settings.HotkeyCode,
                    settings.HotkeyModifiers,
                    ignore);
            }
        }

        private void AutoStartCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            var enabled = AutoStartCheckBox.IsChecked ?? false;
            App.Instance?.SettingsService.SetAutoStart(enabled);
        }

        private void StartMinimizedCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            var minimized = StartMinimizedCheckBox.IsChecked ?? true;
            App.Instance?.SettingsService.UpdateStartMinimized(minimized);
        }

        private void HideButton_Click(object sender, RoutedEventArgs e)
        {
            HideToTray();
        }

        private void ExitButton_Click(object sender, RoutedEventArgs e)
        {
            _trayIcon?.Dispose();
            App.Instance?.ExitApplication();
        }

        #endregion
    }

    public class RelayCommand : System.Windows.Input.ICommand
    {
        private readonly Action _execute;

        public RelayCommand(Action execute)
        {
            _execute = execute;
        }

        public event EventHandler? CanExecuteChanged;

        public bool CanExecute(object? parameter) => true;

        public void Execute(object? parameter) => _execute();
    }
}

