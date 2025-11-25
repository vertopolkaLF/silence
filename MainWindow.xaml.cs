using H.NotifyIcon;
using Microsoft.UI;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
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
        private bool _isHovering;
        private int _maxWindowHeight = 600; // Will be calculated dynamically
        private const int MinWindowWidth = 320;
        private const int TitleBarHeight = 32;

        // Colors for mute button
        private static readonly Windows.UI.Color MutedColor = Windows.UI.Color.FromArgb(255, 205, 60, 70);      // #CD3C46
        private static readonly Windows.UI.Color MutedHoverColor = Windows.UI.Color.FromArgb(255, 160, 40, 50); // Darker red
        private static readonly Windows.UI.Color UnmutedColor = Windows.UI.Color.FromArgb(255, 40, 167, 69);    // #28A745
        private static readonly Windows.UI.Color UnmutedHoverColor = Windows.UI.Color.FromArgb(255, 30, 130, 55); // Darker green

        // Animation duration for hover
        private static readonly TimeSpan AnimationDuration = TimeSpan.FromMilliseconds(200);

        private void SetupTitleBar()
        {
            // Extend content into title bar
            ExtendsContentIntoTitleBar = true;
            
            // Set custom title bar element
            SetTitleBar(AppTitleBar);
            
        }

        private void UpdateTitleBarColors()
        {
            if (_appWindow?.TitleBar == null) return;
            
            var titleBar = _appWindow.TitleBar;
            
            // Check current theme
            var isDarkTheme = (Content as FrameworkElement)?.ActualTheme == ElementTheme.Dark;
            
            if (isDarkTheme)
            {
                // Dark theme - light buttons
                titleBar.ButtonForegroundColor = Colors.White;
                titleBar.ButtonHoverForegroundColor = Colors.White;
                titleBar.ButtonHoverBackgroundColor = Windows.UI.Color.FromArgb(30, 255, 255, 255);
                titleBar.ButtonPressedBackgroundColor = Windows.UI.Color.FromArgb(50, 255, 255, 255);
                titleBar.ButtonPressedForegroundColor = Colors.White;
                titleBar.ButtonInactiveForegroundColor = Windows.UI.Color.FromArgb(128, 255, 255, 255);
            }
            else
            {
                // Light theme - dark buttons
                titleBar.ButtonForegroundColor = Colors.Black;
                titleBar.ButtonHoverForegroundColor = Colors.Black;
                titleBar.ButtonHoverBackgroundColor = Windows.UI.Color.FromArgb(30, 0, 0, 0);
                titleBar.ButtonPressedBackgroundColor = Windows.UI.Color.FromArgb(50, 0, 0, 0);
                titleBar.ButtonPressedForegroundColor = Colors.Black;
                titleBar.ButtonInactiveForegroundColor = Windows.UI.Color.FromArgb(128, 0, 0, 0);
            }
            
            // Transparent background for all states
            titleBar.ButtonBackgroundColor = Colors.Transparent;
            titleBar.ButtonInactiveBackgroundColor = Colors.Transparent;
        }

        private void SetupBackdrop()
        {
            // Check if running on Windows 11 (build 22000+)
            var version = Environment.OSVersion.Version;
            bool isWin11 = version.Build >= 22000;

            if (isWin11 && MicaController.IsSupported())
            {
                // Windows 11 - use Mica
                SystemBackdrop = new MicaBackdrop();
            }
            else if (DesktopAcrylicController.IsSupported())
            {
                // Windows 10 - use Acrylic as fallback
                SystemBackdrop = new DesktopAcrylicBackdrop();
            }
            // else - no backdrop, default solid background
        }

        public MainWindow()
        {
            InitializeComponent();
            
            SetupTitleBar();
            SetupBackdrop();
            SetupWindow();
            SetupTrayIcon();
            LoadSettings();
            
            // Configure title bar caption button colors
            UpdateTitleBarColors();
            
            // Listen for theme changes
            if (Content is FrameworkElement rootElement)
            {
                rootElement.ActualThemeChanged += (s, e) => UpdateTitleBarColors();
            }
            
            // Subscribe to hotkey recording events
            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.KeyPressed += OnKeyPressed;
                App.Instance.KeyboardHookService.ModifiersChanged += OnModifiersChanged;
            }

            this.Closed += MainWindow_Closed;
            UpdateMuteState(App.Instance?.MicrophoneService.IsMuted() ?? false);
        }

        private void SetupWindow()
        {
            var hwnd = WindowNative.GetWindowHandle(this);
            var windowId = Win32Interop.GetWindowIdFromWindow(hwnd);
            _appWindow = AppWindow.GetFromWindowId(windowId);

            if (_appWindow != null)
            {
                const int initialWidth = 400;
                const int initialHeight = 500; // Temporary, will be adjusted
                
                _appWindow.Resize(new SizeInt32(initialWidth, initialHeight));
                
                // Allow horizontal resize, limit vertical to content height
                _appWindow.Changed += (s, e) =>
                {
                    if (e.DidSizeChange && _appWindow.Size.Height > _maxWindowHeight)
                    {
                        _appWindow.Resize(new SizeInt32(_appWindow.Size.Width, _maxWindowHeight));
                    }
                    if (e.DidSizeChange && _appWindow.Size.Width < MinWindowWidth)
                    {
                        _appWindow.Resize(new SizeInt32(MinWindowWidth, _appWindow.Size.Height));
                    }
                };
                
                var displayArea = DisplayArea.GetFromWindowId(windowId, DisplayAreaFallback.Primary);
                if (displayArea != null)
                {
                    var centerX = (displayArea.WorkArea.Width - initialWidth) / 2;
                    var centerY = (displayArea.WorkArea.Height - initialHeight) / 2;
                    _appWindow.Move(new PointInt32(centerX, centerY));
                }

                _appWindow.Title = "silence!";
                
                // Set window icon
                SetWindowIcon();
            }

            // Calculate max height after content is loaded
            if (this.Content is FrameworkElement rootElement)
            {
                rootElement.Loaded += (s, e) =>
                {
                    CalculateMaxWindowHeight();
                };
            }
        }

        private void SetWindowIcon()
        {
            if (_appWindow == null) return;

            try
            {
                var baseDir = AppContext.BaseDirectory;
                System.Diagnostics.Debug.WriteLine($"Base directory: {baseDir}");

                // Load .ico file for title bar (AppWindow.SetIcon supports .ico)
                var iconPath = System.IO.Path.Combine(baseDir, "Assets", "app.ico");
                System.Diagnostics.Debug.WriteLine($"Looking for icon at: {iconPath}");
                
                if (System.IO.File.Exists(iconPath))
                {
                    System.Diagnostics.Debug.WriteLine("Found app.ico, setting icon");
                    _appWindow.SetIcon(iconPath);
                    return;
                }

                // Fallback to PNG if .ico not found
                iconPath = System.IO.Path.Combine(baseDir, "Assets", "app.png");
                System.Diagnostics.Debug.WriteLine($"app.ico not found, trying: {iconPath}");
                
                if (System.IO.File.Exists(iconPath))
                {
                    System.Diagnostics.Debug.WriteLine("Found app.png, setting icon");
                    _appWindow.SetIcon(iconPath);
                    return;
                }

                System.Diagnostics.Debug.WriteLine("No icon files found!");
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Failed to set window icon: {ex.Message}");
                System.Diagnostics.Debug.WriteLine($"Stack trace: {ex.StackTrace}");
            }
        }

        private void CalculateMaxWindowHeight()
        {
            // Measure the actual content panel, not the ScrollViewer
            ContentPanel.Measure(new Windows.Foundation.Size(ContentPanel.ActualWidth > 0 ? ContentPanel.ActualWidth : 400, double.PositiveInfinity));
            var contentHeight = ContentPanel.DesiredSize.Height;
            
            // Add title bar height (32px) + window chrome compensation (16px for Win10 compatibility)
            _maxWindowHeight = (int)Math.Ceiling(contentHeight) + TitleBarHeight + 12;
            
            // Resize window to fit content exactly
            if (_appWindow != null)
            {
                var currentWidth = _appWindow.Size.Width;
                _appWindow.Resize(new SizeInt32(currentWidth, _maxWindowHeight));
                
                // Re-center
                var hwnd = WindowNative.GetWindowHandle(this);
                var windowId = Win32Interop.GetWindowIdFromWindow(hwnd);
                var displayArea = DisplayArea.GetFromWindowId(windowId, DisplayAreaFallback.Primary);
                if (displayArea != null)
                {
                    var centerX = (displayArea.WorkArea.Width - currentWidth) / 2;
                    var centerY = (displayArea.WorkArea.Height - _maxWindowHeight) / 2;
                    _appWindow.Move(new PointInt32(centerX, centerY));
                }
            }
        }

        private MenuFlyout? _trayMenu;

        private void SetupTrayIcon()
        {
            _trayIcon = new TaskbarIcon
            {
                NoLeftClickDelay = true // Instant mute on click
            };
            
            // Create context menu items with commands
            _trayMenu = new MenuFlyout();
            
            var showItem = new MenuFlyoutItem 
            { 
                Text = "Show Settings",
                Command = new RelayCommand(ShowWindow)
            };
            _trayMenu.Items.Add(showItem);

            var muteItem = new MenuFlyoutItem 
            { 
                Text = "Toggle Mute",
                Command = new RelayCommand(() => App.Instance?.ToggleMute())
            };
            _trayMenu.Items.Add(muteItem);

            _trayMenu.Items.Add(new MenuFlyoutSeparator());

            var exitItem = new MenuFlyoutItem 
            { 
                Text = "Exit",
                Command = new RelayCommand(() =>
                {
                    _trayIcon?.Dispose();
                    App.Instance?.ExitApplication();
                })
            };
            _trayMenu.Items.Add(exitItem);

            _trayIcon.ContextFlyout = _trayMenu;
            _trayIcon.LeftClickCommand = new RelayCommand(() => App.Instance?.ToggleMute());
            _trayIcon.DoubleClickCommand = new RelayCommand(() => 
            {
                App.Instance?.ToggleMute(); // Compensate for single click
                ShowWindow();
            });
            _trayIcon.ToolTipText = "silence! - Microphone ON";

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

        private bool _isFirstMuteUpdate = true;
        
        public void UpdateMuteState(bool isMuted)
        {
            var stateChanged = _isMuted != isMuted;
            _isMuted = isMuted;
            
            DispatcherQueue.TryEnqueue(() =>
            {
                if (_isFirstMuteUpdate)
                {
                    // No animation on first load, just set the state
                    _isFirstMuteUpdate = false;
                    MuteStatusText.Text = isMuted ? "muted" : "unmuted";
                    MuteStatusTextAlt.Text = isMuted ? "unmuted" : "muted";
                    MuteStatusText.Opacity = 1;
                    MuteStatusTextAlt.Opacity = 0;
                    MuteStatusTransform.TranslateY = 0;
                    MuteStatusTransformAlt.TranslateY = isMuted ? -30 : 30;
                }
                else if (stateChanged)
                {
                    AnimateMuteText(isMuted);
                }
                UpdateButtonColor();
                UpdateTrayIcon(isMuted);
            });
        }

        private void AnimateMuteText(bool isMuted)
        {
            var storyboard = new Storyboard();
            var duration = TimeSpan.FromMilliseconds(280);
            var easing = new QuadraticEase { EasingMode = EasingMode.EaseInOut };

            // Direction: muted slides UP (disappears up), unmuted slides DOWN (appears from below)
            // When muting: current text goes UP and fades, new "muted" comes from BELOW
            // When unmuting: current text goes DOWN and fades, new "unmuted" comes from ABOVE
            double outDirection = isMuted ? -30 : 30;  // Current text exit direction
            double inStartPos = isMuted ? 30 : -30;    // New text start position

            // Setup new text position and content
            MuteStatusTextAlt.Text = isMuted ? "muted" : "unmuted";
            MuteStatusTransformAlt.TranslateY = inStartPos;
            MuteStatusTransformAlt.ScaleX = 0.8;
            MuteStatusTransformAlt.ScaleY = 0.8;
            
            // === OUTGOING TEXT (MuteStatusText) ===
            
            // Slide out
            var slideOut = new DoubleAnimation
            {
                To = outDirection,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(slideOut, MuteStatusTransform);
            Storyboard.SetTargetProperty(slideOut, "TranslateY");
            storyboard.Children.Add(slideOut);

            // Fade out
            var fadeOut = new DoubleAnimation
            {
                To = 0,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(fadeOut, MuteStatusText);
            Storyboard.SetTargetProperty(fadeOut, "Opacity");
            storyboard.Children.Add(fadeOut);

            // Scale down (blur simulation - elements appear blurry when scaled)
            var scaleOutX = new DoubleAnimation
            {
                To = 0.8,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(scaleOutX, MuteStatusTransform);
            Storyboard.SetTargetProperty(scaleOutX, "ScaleX");
            storyboard.Children.Add(scaleOutX);

            var scaleOutY = new DoubleAnimation
            {
                To = 0.8,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(scaleOutY, MuteStatusTransform);
            Storyboard.SetTargetProperty(scaleOutY, "ScaleY");
            storyboard.Children.Add(scaleOutY);

            // === INCOMING TEXT (MuteStatusTextAlt) ===
            
            // Slide in
            var slideIn = new DoubleAnimation
            {
                To = 0,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(slideIn, MuteStatusTransformAlt);
            Storyboard.SetTargetProperty(slideIn, "TranslateY");
            storyboard.Children.Add(slideIn);

            // Fade in
            var fadeIn = new DoubleAnimation
            {
                To = 1,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(fadeIn, MuteStatusTextAlt);
            Storyboard.SetTargetProperty(fadeIn, "Opacity");
            storyboard.Children.Add(fadeIn);

            // Scale up (blur simulation)
            var scaleInX = new DoubleAnimation
            {
                To = 1,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(scaleInX, MuteStatusTransformAlt);
            Storyboard.SetTargetProperty(scaleInX, "ScaleX");
            storyboard.Children.Add(scaleInX);

            var scaleInY = new DoubleAnimation
            {
                To = 1,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(scaleInY, MuteStatusTransformAlt);
            Storyboard.SetTargetProperty(scaleInY, "ScaleY");
            storyboard.Children.Add(scaleInY);

            // When animation completes, swap the texts
            storyboard.Completed += (s, e) =>
            {
                // Swap: make MuteStatusText the active one again
                MuteStatusText.Text = isMuted ? "muted" : "unmuted";
                MuteStatusText.Opacity = 1;
                MuteStatusTransform.TranslateY = 0;
                MuteStatusTransform.ScaleX = 1;
                MuteStatusTransform.ScaleY = 1;
                
                MuteStatusTextAlt.Opacity = 0;
                MuteStatusTransformAlt.TranslateY = isMuted ? -30 : 30;
            };

            storyboard.Begin();
        }

        private void UpdateButtonColor()
        {
            var color = _isMuted
                ? (_isHovering ? MutedHoverColor : MutedColor)
                : (_isHovering ? UnmutedHoverColor : UnmutedColor);
            
            AnimateButtonColor(color);
        }

        private void AnimateButtonColor(Windows.UI.Color targetColor)
        {
            // Ensure brush exists for animation
            var currentBrush = MuteButton.Background as SolidColorBrush;
            if (currentBrush == null)
            {
                MuteButton.Background = new SolidColorBrush(targetColor);
                return;
            }

            // Smooth color transition with Storyboard
            var storyboard = new Storyboard();
            var animation = new ColorAnimation
            {
                To = targetColor,
                Duration = new Duration(AnimationDuration),
                EasingFunction = new QuadraticEase { EasingMode = EasingMode.EaseOut }
            };

            Storyboard.SetTarget(animation, MuteButton);
            Storyboard.SetTargetProperty(animation, "(Border.Background).(SolidColorBrush.Color)");
            storyboard.Children.Add(animation);
            storyboard.Begin();
        }

        private void MuteButton_PointerEntered(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            _isHovering = true;
            UpdateButtonColor();
        }

        private void MuteButton_PointerExited(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            _isHovering = false;
            UpdateButtonColor();
        }

        private void ContentScrollViewer_SizeChanged(object sender, SizeChangedEventArgs e)
        {
            UpdateScrollBarVisibility();
        }

        private void UpdateScrollBarVisibility()
        {
            // Measure content to see if it needs scrolling
            ContentPanel.Measure(new Windows.Foundation.Size(ContentPanel.ActualWidth > 0 ? ContentPanel.ActualWidth : 400, double.PositiveInfinity));
            var contentHeight = ContentPanel.DesiredSize.Height;
            var availableHeight = ContentScrollViewer.ActualHeight;

            // Enable scrolling only if content is taller than available space
            if (contentHeight > availableHeight + 1) // +1 for float precision
            {
                ContentScrollViewer.VerticalScrollBarVisibility = ScrollBarVisibility.Auto;
            }
            else
            {
                ContentScrollViewer.VerticalScrollBarVisibility = ScrollBarVisibility.Disabled;
            }
        }

        private void UpdateTrayIcon(bool isMuted)
        {
            if (_trayIcon == null) return;

            try
            {
                var icon = CreateMicrophoneIcon(isMuted);
                _trayIcon.Icon = icon;
                _trayIcon.ToolTipText = isMuted ? "silence! - Microphone MUTED" : "silence! - Microphone ON";
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

        private void MuteButton_Click(object sender, Microsoft.UI.Xaml.Input.TappedRoutedEventArgs e)
        {
            App.Instance?.ToggleMute();
        }

        private void MuteButton_PointerPressed(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            // Slightly darker on press
            _isHovering = true;
            UpdateButtonColor();
        }

        private void MuteButton_PointerReleased(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            UpdateButtonColor();
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

