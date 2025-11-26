using H.NotifyIcon;
using Microsoft.UI;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using silence_.Pages;
using silence_.Services;
using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Runtime.InteropServices;
using Windows.Graphics;
using WinRT.Interop;

namespace silence_
{
    public sealed partial class MainWindow : Window
    {
        private TaskbarIcon? _trayIcon;
        private AppWindow? _appWindow;
        private const int MinWindowWidth = 580;
        private const int MinWindowHeight = 480;

        private string _currentPage = "General";
        private bool _updateAvailable = false;

        private void SetupTitleBar()
        {
            ExtendsContentIntoTitleBar = true;
            SetTitleBar(AppTitleBar);
        }

        private void UpdateTitleBarColors()
        {
            if (_appWindow?.TitleBar == null) return;
            
            var titleBar = _appWindow.TitleBar;
            var isDarkTheme = (Content as FrameworkElement)?.ActualTheme == ElementTheme.Dark;
            
            if (isDarkTheme)
            {
                titleBar.ButtonForegroundColor = Colors.White;
                titleBar.ButtonHoverForegroundColor = Colors.White;
                titleBar.ButtonHoverBackgroundColor = Windows.UI.Color.FromArgb(30, 255, 255, 255);
                titleBar.ButtonPressedBackgroundColor = Windows.UI.Color.FromArgb(50, 255, 255, 255);
                titleBar.ButtonPressedForegroundColor = Colors.White;
                titleBar.ButtonInactiveForegroundColor = Windows.UI.Color.FromArgb(128, 255, 255, 255);
            }
            else
            {
                titleBar.ButtonForegroundColor = Colors.Black;
                titleBar.ButtonHoverForegroundColor = Colors.Black;
                titleBar.ButtonHoverBackgroundColor = Windows.UI.Color.FromArgb(30, 0, 0, 0);
                titleBar.ButtonPressedBackgroundColor = Windows.UI.Color.FromArgb(50, 0, 0, 0);
                titleBar.ButtonPressedForegroundColor = Colors.Black;
                titleBar.ButtonInactiveForegroundColor = Windows.UI.Color.FromArgb(128, 0, 0, 0);
            }
            
            titleBar.ButtonBackgroundColor = Colors.Transparent;
            titleBar.ButtonInactiveBackgroundColor = Colors.Transparent;
        }

        private void SetupBackdrop()
        {
            var version = Environment.OSVersion.Version;
            bool isWin11 = version.Build >= 22000;

            if (isWin11 && MicaController.IsSupported())
            {
                SystemBackdrop = new MicaBackdrop();
            }
            else if (DesktopAcrylicController.IsSupported())
            {
                SystemBackdrop = new DesktopAcrylicBackdrop();
            }
        }

        public MainWindow()
        {
            InitializeComponent();
            
            SetupTitleBar();
            SetupBackdrop();
            SetupWindow();
            SetupTrayIcon();
            
            UpdateTitleBarColors();
            
            if (Content is FrameworkElement rootElement)
            {
                rootElement.ActualThemeChanged += (s, e) => UpdateTitleBarColors();
            }

            // Navigate to General page initially
            ContentFrame.Navigate(typeof(GeneralPage), null, new SuppressNavigationTransitionInfo());

            this.Closed += MainWindow_Closed;
            
            // Subscribe to mute state changes for tray icon
            if (App.Instance != null)
            {
                App.Instance.MuteStateChanged += OnMuteStateChanged;
                App.Instance.UpdateAvailable += OnUpdateAvailable;
            }
            UpdateTrayIcon(App.Instance?.MicrophoneService.IsMuted() ?? false);
        }
        
        private void OnUpdateAvailable(UpdateCheckResult result)
        {
            DispatcherQueue.TryEnqueue(() =>
            {
                _updateAvailable = true;
                UpdatePlaceholder.Visibility = Visibility.Collapsed;
                
                // Show appropriate notification based on pane state
                if (NavView.IsPaneOpen)
                {
                    UpdateNotificationBorder.Visibility = Visibility.Visible;
                    UpdateNotificationCompact.Visibility = Visibility.Collapsed;
                }
                else
                {
                    UpdateNotificationBorder.Visibility = Visibility.Collapsed;
                    UpdateNotificationCompact.Visibility = Visibility.Visible;
                }
            });
        }
        
        private void UpdateInfoBar_ActionClick(object sender, RoutedEventArgs e)
        {
            NavigateToAboutAndHideNotification();
        }
        
        private void UpdateNotificationCompact_Click(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            NavigateToAboutAndHideNotification();
        }
        
        private void UpdateNotificationCompact_PointerEntered(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            // Use the same hover color as NavigationViewItem
            if (Application.Current.Resources.TryGetValue("NavigationViewItemBackgroundPointerOver", out var brush))
            {
                UpdateNotificationCompact.Background = brush as Microsoft.UI.Xaml.Media.Brush;
            }
            else
            {
                // Fallback - matches NavigationView hover
                UpdateNotificationCompact.Background = new SolidColorBrush(
                    (Content as FrameworkElement)?.ActualTheme == ElementTheme.Dark
                        ? Windows.UI.Color.FromArgb(15, 255, 255, 255)
                        : Windows.UI.Color.FromArgb(15, 0, 0, 0));
            }
        }
        
        private void UpdateNotificationCompact_PointerExited(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
            UpdateNotificationCompact.Background = new SolidColorBrush(Colors.Transparent);
        }
        
        private void NavigateToAboutAndHideNotification()
        {
            // Navigate to About page to show update details
            NavView.SelectedItem = AboutNavItem;
            _updateAvailable = false;
            UpdateNotificationBorder.Visibility = Visibility.Collapsed;
            UpdateNotificationCompact.Visibility = Visibility.Collapsed;
            UpdatePlaceholder.Visibility = Visibility.Visible;
        }

        private void OnMuteStateChanged(bool isMuted)
        {
            DispatcherQueue.TryEnqueue(() => UpdateTrayIcon(isMuted));
        }

        private void SetupWindow()
        {
            var hwnd = WindowNative.GetWindowHandle(this);
            var windowId = Win32Interop.GetWindowIdFromWindow(hwnd);
            _appWindow = AppWindow.GetFromWindowId(windowId);

            if (_appWindow != null)
            {
                const int initialWidth = 580;
                const int initialHeight = 480;
                
                _appWindow.Resize(new SizeInt32(initialWidth, initialHeight));
                
                _appWindow.Changed += (s, e) =>
                {
                    if (e.DidSizeChange && _appWindow.Size.Width < MinWindowWidth)
                    {
                        _appWindow.Resize(new SizeInt32(MinWindowWidth, _appWindow.Size.Height));
                    }
                    if (e.DidSizeChange && _appWindow.Size.Height < MinWindowHeight)
                    {
                        _appWindow.Resize(new SizeInt32(_appWindow.Size.Width, MinWindowHeight));
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
                SetWindowIcon();
            }
        }

        private void SetWindowIcon()
        {
            if (_appWindow == null) return;

            try
            {
                var baseDir = AppContext.BaseDirectory;
                var iconPath = System.IO.Path.Combine(baseDir, "Assets", "app.ico");
                
                if (System.IO.File.Exists(iconPath))
                {
                    _appWindow.SetIcon(iconPath);
                    return;
                }

                iconPath = System.IO.Path.Combine(baseDir, "Assets", "app.png");
                if (System.IO.File.Exists(iconPath))
                {
                    _appWindow.SetIcon(iconPath);
                }
            }
            catch (Exception ex)
            {
                System.Diagnostics.Debug.WriteLine($"Failed to set window icon: {ex.Message}");
            }
        }

        private MenuFlyout? _trayMenu;

        private void SetupTrayIcon()
        {
            _trayIcon = new TaskbarIcon
            {
                NoLeftClickDelay = false // Add delay to handle double-click properly
            };
            
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
            _trayIcon.DoubleClickCommand = new RelayCommand(ShowWindow);
            _trayIcon.ToolTipText = "silence! - Microphone ON";

            UpdateTrayIcon(false);
            _trayIcon.ForceCreate();
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

        private void NavView_PaneOpening(NavigationView sender, object args)
        {
            // Switch to expanded update notification
            if (_updateAvailable)
            {
                UpdateNotificationBorder.Visibility = Visibility.Visible;
                UpdateNotificationCompact.Visibility = Visibility.Collapsed;
            }
        }

        private void NavView_PaneClosing(NavigationView sender, NavigationViewPaneClosingEventArgs args)
        {
            // Switch to compact update notification (icon only)
            if (_updateAvailable)
            {
                UpdateNotificationBorder.Visibility = Visibility.Collapsed;
                UpdateNotificationCompact.Visibility = Visibility.Visible;
            }
        }

        private void NavView_SelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
        {
            if (args.SelectedItem is NavigationViewItem item)
            {
                var tag = item.Tag?.ToString();
                if (tag == null || tag == _currentPage) return;

                // Determine slide direction
                var pageOrder = new[] { "General", "Sounds", "Appearance", "About" };
                var currentIndex = Array.IndexOf(pageOrder, _currentPage);
                var newIndex = Array.IndexOf(pageOrder, tag);
                var effect = newIndex > currentIndex 
                    ? SlideNavigationTransitionEffect.FromRight 
                    : SlideNavigationTransitionEffect.FromLeft;

                Type? pageType = tag switch
                {
                    "General" => typeof(GeneralPage),
                    "Sounds" => typeof(SoundsPage),
                    "Appearance" => typeof(AppearancePage),
                    "About" => typeof(AboutPage),
                    _ => null
                };

                if (pageType != null)
                {
                    ContentFrame.Navigate(pageType, null, new SlideNavigationTransitionInfo { Effect = effect });
                    _currentPage = tag;
                }
            }
        }

        private void MainWindow_Closed(object sender, WindowEventArgs args)
        {
            args.Handled = true;
            HideToTray();
        }

        public void HideToTray()
        {
            _appWindow?.Hide();
        }

        public void ShowWindow()
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

        public void DisposeTrayIcon()
        {
            _trayIcon?.Dispose();
        }
    }

    public class RelayCommand : System.Windows.Input.ICommand
    {
        private readonly Action _execute;

        public RelayCommand(Action execute)
        {
            _execute = execute;
        }

        #pragma warning disable CS0067 // Event is never used
        public event EventHandler? CanExecuteChanged;
#pragma warning restore CS0067

        public bool CanExecute(object? parameter) => true;

        public void Execute(object? parameter) => _execute();
    }
}
