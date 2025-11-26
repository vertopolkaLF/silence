using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using silence_.Services;
using System;
using System.Collections.ObjectModel;
using System.IO;
using System.Linq;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace silence_.Pages
{
    public sealed partial class SoundsPage : Page
    {
        private ObservableCollection<CustomSound> _customSounds = new();
        private bool _isInitializing = true;

        public SoundsPage()
        {
            InitializeComponent();
            CustomSoundsListView.ItemsSource = _customSounds;
            LoadSettings();
            _isInitializing = false;
        }

        private void LoadSettings()
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null) return;

            // Load sounds enabled state
            SoundsEnabledToggle.IsOn = settings.SoundsEnabled;
            UpdateSoundPanelsVisibility(settings.SoundsEnabled);

            // Load volume
            VolumeSlider.Value = settings.SoundVolume * 100;
            VolumePercentText.Text = $"{(int)(settings.SoundVolume * 100)}%";

            // Populate comboboxes
            PopulateSoundComboBoxes();

            // Load custom sounds list
            RefreshCustomSoundsList();

            // Set selected sounds
            SelectSoundInComboBox(MuteSoundComboBox, settings.MuteSoundPreloaded, settings.MuteSoundCustomPath, true);
            SelectSoundInComboBox(UnmuteSoundComboBox, settings.UnmuteSoundPreloaded, settings.UnmuteSoundCustomPath, false);
        }

        private void PopulateSoundComboBoxes()
        {
            PopulateSoundComboBox(MuteSoundComboBox, true);
            PopulateSoundComboBox(UnmuteSoundComboBox, false);
        }

        private void PopulateSoundComboBox(ComboBox comboBox, bool isMute)
        {
            comboBox.Items.Clear();

            // Add preloaded sounds
            foreach (var sound in SoundService.PreloadedSounds)
            {
                comboBox.Items.Add(new ComboBoxItem
                {
                    Content = sound.DisplayName,
                    Tag = new SoundSelection { Type = SoundType.Preloaded, Key = sound.Key }
                });
            }

            // Add separator if there are custom sounds
            var customSounds = App.Instance?.SoundService?.GetCustomSounds();
            if (customSounds?.Count > 0)
            {
                comboBox.Items.Add(new ComboBoxItem
                {
                    Content = "─────────────",
                    IsEnabled = false,
                    Tag = null
                });

                foreach (var sound in customSounds)
                {
                    comboBox.Items.Add(new ComboBoxItem
                    {
                        Content = $"🎵 {sound.DisplayName}",
                        Tag = new SoundSelection { Type = SoundType.Custom, Path = sound.FilePath }
                    });
                }
            }

            // Add "Browse..." option at the end
            comboBox.Items.Add(new ComboBoxItem
            {
                Content = "─────────────",
                IsEnabled = false,
                Tag = null
            });
            comboBox.Items.Add(new ComboBoxItem
            {
                Content = "📁 Browse for file...",
                Tag = new SoundSelection { Type = SoundType.Browse }
            });
        }

        private void SelectSoundInComboBox(ComboBox comboBox, string? preloadedKey, string? customPath, bool isMute)
        {
            // Show custom sound indicator if using custom path
            var customGrid = isMute ? CustomMuteSoundGrid : CustomUnmuteSoundGrid;
            var customText = isMute ? CustomMuteSoundText : CustomUnmuteSoundText;

            if (!string.IsNullOrEmpty(customPath))
            {
                customGrid.Visibility = Visibility.Visible;
                customText.Text = $"Custom: {Path.GetFileName(customPath)}";

                // Select the custom sound in combo if it exists
                foreach (ComboBoxItem item in comboBox.Items)
                {
                    if (item.Tag is SoundSelection sel && sel.Type == SoundType.Custom && sel.Path == customPath)
                    {
                        comboBox.SelectedItem = item;
                        return;
                    }
                }

                // Custom path not in list anymore, select "None" and show indicator
                comboBox.SelectedIndex = 0;
                return;
            }

            customGrid.Visibility = Visibility.Collapsed;

            // Select preloaded sound
            foreach (ComboBoxItem item in comboBox.Items)
            {
                if (item.Tag is SoundSelection sel && sel.Type == SoundType.Preloaded && sel.Key == preloadedKey)
                {
                    comboBox.SelectedItem = item;
                    return;
                }
            }

            // Default to first item (None)
            comboBox.SelectedIndex = 0;
        }

        private void RefreshCustomSoundsList()
        {
            _customSounds.Clear();
            var sounds = App.Instance?.SoundService?.GetCustomSounds();
            if (sounds != null)
            {
                foreach (var sound in sounds)
                {
                    _customSounds.Add(sound);
                }
            }
        }

        private void UpdateSoundPanelsVisibility(bool enabled)
        {
            VolumePanel.Opacity = enabled ? 1.0 : 0.5;
            MuteSoundPanel.Opacity = enabled ? 1.0 : 0.5;
            UnmuteSoundPanel.Opacity = enabled ? 1.0 : 0.5;
            VolumePanel.IsHitTestVisible = enabled;
            MuteSoundPanel.IsHitTestVisible = enabled;
            UnmuteSoundPanel.IsHitTestVisible = enabled;
        }

        #region Event Handlers

        private void SoundsEnabledToggle_Toggled(object sender, RoutedEventArgs e)
        {
            if (_isInitializing) return;
            
            var enabled = SoundsEnabledToggle.IsOn;
            App.Instance?.SettingsService.UpdateSoundsEnabled(enabled);
            UpdateSoundPanelsVisibility(enabled);
        }

        private void VolumeSlider_ValueChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e)
        {
            // VolumePercentText might be null during initial XAML parsing
            if (VolumePercentText == null) return;
            
            var volumePercent = (int)VolumeSlider.Value;
            VolumePercentText.Text = $"{volumePercent}%";
            
            if (_isInitializing) return;
            
            var volume = (float)(VolumeSlider.Value / 100.0);
            App.Instance?.SettingsService.UpdateSoundVolume(volume);
        }

        private async void MuteSoundComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (_isInitializing) return;
            if (MuteSoundComboBox.SelectedItem is not ComboBoxItem item) return;
            if (item.Tag is not SoundSelection selection) return;

            if (selection.Type == SoundType.Browse)
            {
                await BrowseForSoundFile(true);
                return;
            }

            if (selection.Type == SoundType.Preloaded)
            {
                App.Instance?.SettingsService.UpdateMuteSound(selection.Key, null);
                CustomMuteSoundGrid.Visibility = Visibility.Collapsed;
            }
            else if (selection.Type == SoundType.Custom && selection.Path != null)
            {
                App.Instance?.SettingsService.UpdateMuteSound(null, selection.Path);
                CustomMuteSoundGrid.Visibility = Visibility.Visible;
                CustomMuteSoundText.Text = $"Custom: {Path.GetFileName(selection.Path)}";
            }
        }

        private async void UnmuteSoundComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (_isInitializing) return;
            if (UnmuteSoundComboBox.SelectedItem is not ComboBoxItem item) return;
            if (item.Tag is not SoundSelection selection) return;

            if (selection.Type == SoundType.Browse)
            {
                await BrowseForSoundFile(false);
                return;
            }

            if (selection.Type == SoundType.Preloaded)
            {
                App.Instance?.SettingsService.UpdateUnmuteSound(selection.Key, null);
                CustomUnmuteSoundGrid.Visibility = Visibility.Collapsed;
            }
            else if (selection.Type == SoundType.Custom && selection.Path != null)
            {
                App.Instance?.SettingsService.UpdateUnmuteSound(null, selection.Path);
                CustomUnmuteSoundGrid.Visibility = Visibility.Visible;
                CustomUnmuteSoundText.Text = $"Custom: {Path.GetFileName(selection.Path)}";
            }
        }

        private void PlayMuteSoundButton_Click(object sender, RoutedEventArgs e)
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null) return;

            var path = App.Instance?.SoundService?.GetSoundPath(
                settings.MuteSoundPreloaded, 
                settings.MuteSoundCustomPath, 
                true);
            App.Instance?.SoundService?.PlaySound(path, settings.SoundVolume);
        }

        private void PlayUnmuteSoundButton_Click(object sender, RoutedEventArgs e)
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null) return;

            var path = App.Instance?.SoundService?.GetSoundPath(
                settings.UnmuteSoundPreloaded, 
                settings.UnmuteSoundCustomPath, 
                false);
            App.Instance?.SoundService?.PlaySound(path, settings.SoundVolume);
        }

        private void RemoveMuteCustomSoundButton_Click(object sender, RoutedEventArgs e)
        {
            App.Instance?.SettingsService.UpdateMuteSound("sifi", null);
            CustomMuteSoundGrid.Visibility = Visibility.Collapsed;
            
            // Re-select preloaded sound
            _isInitializing = true;
            SelectSoundInComboBox(MuteSoundComboBox, "sifi", null, true);
            _isInitializing = false;
        }

        private void RemoveUnmuteCustomSoundButton_Click(object sender, RoutedEventArgs e)
        {
            App.Instance?.SettingsService.UpdateUnmuteSound("sifi", null);
            CustomUnmuteSoundGrid.Visibility = Visibility.Collapsed;
            
            // Re-select preloaded sound
            _isInitializing = true;
            SelectSoundInComboBox(UnmuteSoundComboBox, "sifi", null, false);
            _isInitializing = false;
        }

        private async void AddCustomSoundButton_Click(object sender, RoutedEventArgs e)
        {
            var picker = new FileOpenPicker();
            picker.SuggestedStartLocation = PickerLocationId.MusicLibrary;
            picker.FileTypeFilter.Add(".mp3");
            picker.FileTypeFilter.Add(".wav");
            picker.FileTypeFilter.Add(".flac");
            picker.FileTypeFilter.Add(".ogg");
            picker.FileTypeFilter.Add(".m4a");
            picker.FileTypeFilter.Add(".wma");

            // Get window handle for the picker
            var hwnd = WindowNative.GetWindowHandle(App.Instance?.MainWindowInstance);
            InitializeWithWindow.Initialize(picker, hwnd);

            var file = await picker.PickSingleFileAsync();
            if (file == null) return;

            var soundService = App.Instance?.SoundService;
            if (soundService == null) return;

            var addedPath = await soundService.AddCustomSoundAsync(file.Path);
            if (addedPath != null)
            {
                RefreshCustomSoundsList();
                
                // Refresh comboboxes to include new sound
                _isInitializing = true;
                var settings = App.Instance?.SettingsService.Settings;
                PopulateSoundComboBoxes();
                SelectSoundInComboBox(MuteSoundComboBox, settings?.MuteSoundPreloaded, settings?.MuteSoundCustomPath, true);
                SelectSoundInComboBox(UnmuteSoundComboBox, settings?.UnmuteSoundPreloaded, settings?.UnmuteSoundCustomPath, false);
                _isInitializing = false;
            }
        }

        private async System.Threading.Tasks.Task BrowseForSoundFile(bool isMute)
        {
            var picker = new FileOpenPicker();
            picker.SuggestedStartLocation = PickerLocationId.MusicLibrary;
            picker.FileTypeFilter.Add(".mp3");
            picker.FileTypeFilter.Add(".wav");
            picker.FileTypeFilter.Add(".flac");
            picker.FileTypeFilter.Add(".ogg");
            picker.FileTypeFilter.Add(".m4a");
            picker.FileTypeFilter.Add(".wma");

            var hwnd = WindowNative.GetWindowHandle(App.Instance?.MainWindowInstance);
            InitializeWithWindow.Initialize(picker, hwnd);

            var file = await picker.PickSingleFileAsync();
            
            _isInitializing = true;
            
            var soundService = App.Instance?.SoundService;
            if (file != null && soundService != null)
            {
                // Add to custom sounds and use it
                var addedPath = await soundService.AddCustomSoundAsync(file.Path);
                if (addedPath != null)
                {
                    RefreshCustomSoundsList();
                    
                    // Refresh comboboxes
                    PopulateSoundComboBoxes();
                    
                    // Set as current sound
                    if (isMute)
                    {
                        App.Instance?.SettingsService.UpdateMuteSound(null, addedPath);
                        SelectSoundInComboBox(MuteSoundComboBox, null, addedPath, true);
                    }
                    else
                    {
                        App.Instance?.SettingsService.UpdateUnmuteSound(null, addedPath);
                        SelectSoundInComboBox(UnmuteSoundComboBox, null, addedPath, false);
                    }
                    
                    _isInitializing = false;
                    return;
                }
            }
            
            // Revert selection if cancelled or failed
            var settings = App.Instance?.SettingsService.Settings;
            if (isMute)
            {
                SelectSoundInComboBox(MuteSoundComboBox, settings?.MuteSoundPreloaded, settings?.MuteSoundCustomPath, true);
            }
            else
            {
                SelectSoundInComboBox(UnmuteSoundComboBox, settings?.UnmuteSoundPreloaded, settings?.UnmuteSoundCustomPath, false);
            }
            
            _isInitializing = false;
        }

        private void PlayCustomSoundButton_Click(object sender, RoutedEventArgs e)
        {
            if (sender is Button button && button.DataContext is CustomSound sound)
            {
                var volume = App.Instance?.SettingsService.Settings.SoundVolume ?? 0.5f;
                App.Instance?.SoundService?.PlaySound(sound.FilePath, volume);
            }
        }

        private void DeleteCustomSoundButton_Click(object sender, RoutedEventArgs e)
        {
            if (sender is Button button && button.DataContext is CustomSound sound)
            {
                var deleted = App.Instance?.SoundService?.DeleteCustomSound(sound.FilePath) ?? false;
                if (deleted)
                {
                    _customSounds.Remove(sound);
                    
                    // Check if this sound was selected and reset if so
                    var settings = App.Instance?.SettingsService.Settings;
                    if (settings?.MuteSoundCustomPath == sound.FilePath)
                    {
                        App.Instance?.SettingsService.UpdateMuteSound("sifi", null);
                    }
                    if (settings?.UnmuteSoundCustomPath == sound.FilePath)
                    {
                        App.Instance?.SettingsService.UpdateUnmuteSound("sifi", null);
                    }
                    
                    // Refresh comboboxes
                    _isInitializing = true;
                    settings = App.Instance?.SettingsService.Settings;
                    PopulateSoundComboBoxes();
                    SelectSoundInComboBox(MuteSoundComboBox, settings?.MuteSoundPreloaded, settings?.MuteSoundCustomPath, true);
                    SelectSoundInComboBox(UnmuteSoundComboBox, settings?.UnmuteSoundPreloaded, settings?.UnmuteSoundCustomPath, false);
                    _isInitializing = false;
                }
            }
        }

        #endregion
    }

    internal enum SoundType
    {
        Preloaded,
        Custom,
        Browse
    }

    internal class SoundSelection
    {
        public SoundType Type { get; set; }
        public string? Key { get; set; }  // For preloaded sounds
        public string? Path { get; set; } // For custom sounds
    }
}

