using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.UI;

namespace silence_.Pages
{
    public sealed partial class AppearancePage : Page
    {
        private bool _isInitializing = true;
        private static readonly Color MutedColor = Color.FromArgb(255, 220, 53, 69);
        private static readonly Color UnmutedColor = Color.FromArgb(255, 40, 167, 69);

        public AppearancePage()
        {
            InitializeComponent();
            LoadSettings();
            UpdatePreviewState(App.Instance?.MicrophoneService.IsMuted() ?? false);
            _isInitializing = false;

            if (App.Instance != null)
            {
                App.Instance.MuteStateChanged += OnMuteStateChanged;
            }
        }

        private void LoadSettings()
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null)
            {
                UpdateVariantSelection("Standard");
                return;
            }

            UpdateVariantSelection(settings.TrayIconStyle);
        }

        private void OnMuteStateChanged(bool isMuted)
        {
            DispatcherQueue.TryEnqueue(() => UpdatePreviewState(isMuted));
        }

        private void TrayIconVariantButton_Click(object sender, RoutedEventArgs e)
        {
            if (_isInitializing || sender is not Button button) return;

            var style = button.Tag?.ToString() ?? "Standard";
            App.Instance?.SettingsService.UpdateTrayIconStyle(style);
            UpdateVariantSelection(style);
            App.Instance?.MainWindowInstance?.RefreshTrayIcon();
        }

        private void UpdateVariantSelection(string style)
        {
            var accentBrush = Application.Current.Resources["AccentFillColorDefaultBrush"] as Brush;
            var defaultBrush = Application.Current.Resources["CardStrokeColorDefaultBrush"] as Brush;

            StandardVariantBorder.BorderBrush = style == "Standard" ? accentBrush : defaultBrush;
            FilledCircleVariantBorder.BorderBrush = style == "FilledCircle" ? accentBrush : defaultBrush;
            DotVariantBorder.BorderBrush = style == "Dot" ? accentBrush : defaultBrush;
        }

        private void UpdatePreviewState(bool isMuted)
        {
            var color = isMuted ? MutedColor : UnmutedColor;
            var brush = new SolidColorBrush(color);

            StandardMicBodyPath.Fill = brush;
            StandardMicArcPath.Stroke = brush;
            StandardMicStemLine.Stroke = brush;
            StandardMicBaseLine.Stroke = brush;
            FilledCirclePreview.Fill = brush;
            DotPreview.Fill = brush;
        }

        protected override void OnNavigatedFrom(Microsoft.UI.Xaml.Navigation.NavigationEventArgs e)
        {
            base.OnNavigatedFrom(e);

            if (App.Instance != null)
            {
                App.Instance.MuteStateChanged -= OnMuteStateChanged;
            }
        }
    }
}

