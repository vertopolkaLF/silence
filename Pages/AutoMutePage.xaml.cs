using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using silence_.Services;
using System;

namespace silence_.Pages
{
    public sealed partial class AutoMutePage : Page
    {
        private readonly bool _isInitializing;

        public AutoMutePage()
        {
            _isInitializing = true;
            InitializeComponent();
            ApplyLocalizedStrings();
            LoadSettings();
            _isInitializing = false;
        }

        private void ApplyLocalizedStrings()
        {
            TitleTextBlock.Text = AppResources.GetString("AutoMutePage.TitleText.Text");
            DescriptionTextBlock.Text = AppResources.GetString("AutoMutePage.DescriptionText.Text");
            StartupLabelText.Text = AppResources.GetString("AutoMutePage.StartupLabel.Text");
            MuteOnStartupCheckBox.Content = AppResources.GetString("AutoMutePage.MuteOnStartupCheckBox.Content");
            InactivityLabelText.Text = AppResources.GetString("AutoMutePage.InactivityLabel.Text");
            MuteAfterInactivityCheckBox.Content = AppResources.GetString("AutoMutePage.MuteAfterInactivityCheckBox.Content");
            InactivityMinutesLabelText.Text = AppResources.GetString("AutoMutePage.InactivityMinutesLabel.Text");
            InactivityDescriptionTextBlock.Text = AppResources.GetString("AutoMutePage.InactivityDescriptionText.Text");
            UnmuteOnActivityCheckBox.Content = AppResources.GetString("AutoMutePage.UnmuteOnActivityCheckBox.Content");
            OptionsLabelText.Text = AppResources.GetString("AutoMutePage.OptionsLabel.Text");
            PlaySoundsOnAutoMuteCheckBox.Content = AppResources.GetString("AutoMutePage.PlaySoundsOnAutoMuteCheckBox.Content");
        }

        private void LoadSettings()
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null)
            {
                return;
            }

            MuteOnStartupCheckBox.IsChecked = settings.AutoMuteOnStartup;
            MuteAfterInactivityCheckBox.IsChecked = settings.AutoMuteAfterInactivityEnabled;
            InactivityMinutesNumberBox.Value = settings.AutoMuteAfterInactivityMinutes;
            UnmuteOnActivityCheckBox.IsChecked = settings.AutoUnmuteOnActivity;
            PlaySoundsOnAutoMuteCheckBox.IsChecked = settings.AutoMutePlaySounds;

            UpdateInactivitySettingsState(settings.AutoMuteAfterInactivityEnabled);
        }

        private void UpdateInactivitySettingsState(bool isEnabled)
        {
            InactivitySettingsPanel.Opacity = isEnabled ? 1.0 : 0.5;
            InactivitySettingsPanel.IsHitTestVisible = isEnabled;
        }

        private void MuteOnStartupCheckBox_Changed(object _, RoutedEventArgs __)
        {
            if (_isInitializing) return;

            var enabled = MuteOnStartupCheckBox.IsChecked ?? false;
            App.Instance?.SettingsService.UpdateAutoMuteOnStartup(enabled);
        }

        private void MuteAfterInactivityCheckBox_Changed(object _, RoutedEventArgs __)
        {
            if (_isInitializing) return;

            var enabled = MuteAfterInactivityCheckBox.IsChecked ?? false;
            UpdateInactivitySettingsState(enabled);
            App.Instance?.SettingsService.UpdateAutoMuteAfterInactivityEnabled(enabled);
            App.Instance?.RefreshAutoMuteMonitoring();
        }

        private void InactivityMinutesNumberBox_ValueChanged(NumberBox sender, NumberBoxValueChangedEventArgs _)
        {
            if (_isInitializing || double.IsNaN(sender.Value)) return;

            var minutes = Math.Clamp((int)Math.Round(sender.Value), 1, 1440);
            if (Math.Abs(sender.Value - minutes) > double.Epsilon)
            {
                sender.Value = minutes;
            }

            App.Instance?.SettingsService.UpdateAutoMuteAfterInactivityMinutes(minutes);
            App.Instance?.RefreshAutoMuteMonitoring();
        }

        private void PlaySoundsOnAutoMuteCheckBox_Changed(object _, RoutedEventArgs __)
        {
            if (_isInitializing) return;

            var enabled = PlaySoundsOnAutoMuteCheckBox.IsChecked ?? true;
            App.Instance?.SettingsService.UpdateAutoMutePlaySounds(enabled);
        }

        private void UnmuteOnActivityCheckBox_Changed(object _, RoutedEventArgs __)
        {
            if (_isInitializing) return;

            var enabled = UnmuteOnActivityCheckBox.IsChecked ?? false;
            App.Instance?.SettingsService.UpdateAutoUnmuteOnActivity(enabled);
            App.Instance?.RefreshAutoMuteMonitoring();
        }
    }
}
