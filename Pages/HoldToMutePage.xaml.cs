using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using silence_.Services;
using System;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace silence_.Pages
{
    public sealed partial class HoldToMutePage : Page
    {
        private bool _isRecordingHoldHotkey;
        private int _recordedKeyCode;
        private ModifierKeys _recordedModifiers;
        private bool _isInitializing;

        public HoldToMutePage()
        {
            InitializeComponent();
            LoadSettings();
            
            // Subscribe to hotkey recording events
            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.KeyPressed += OnKeyPressed;
                App.Instance.KeyboardHookService.ModifiersChanged += OnModifiersChanged;
                App.Instance.KeyboardHookService.ModifierHoldProgress += OnModifierHoldProgress;
            }
        }

        private void LoadSettings()
        {
            _isInitializing = true;
            
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null) return;

            // Load hold hotkey settings
            if (settings.HoldHotkeyCode > 0 || settings.HoldHotkeyModifiers != ModifierKeys.None)
            {
                HoldHotkeyTextBox.Text = VirtualKeys.GetHotkeyDisplayString(settings.HoldHotkeyCode, settings.HoldHotkeyModifiers);
            }
            else
            {
                HoldHotkeyTextBox.Text = "";
            }
            IgnoreHoldModifiersCheckBox.IsChecked = settings.IgnoreHoldModifiers;
            HoldPlaySoundsCheckBox.IsChecked = settings.HoldPlaySounds;
            HoldShowOverlayCheckBox.IsChecked = settings.HoldShowOverlay;
            
            // Load action mode
            var actionIndex = settings.HoldAction switch
            {
                "Toggle" => 0,
                "HoldToMute" => 1,
                "HoldToUnmute" => 2,
                _ => 0
            };
            HoldActionComboBox.SelectedIndex = actionIndex;
            UpdateActionDescription(settings.HoldAction);
            
            // Load sound settings
            PopulateSoundComboBoxes();
            LoadSoundSettings();
            
            _isInitializing = false;
        }

        private void PopulateSoundComboBoxes()
        {
            PopulateSoundComboBox(HoldMuteSoundComboBox);
            PopulateSoundComboBox(HoldUnmuteSoundComboBox);
        }

        private void PopulateSoundComboBox(ComboBox comboBox)
        {
            comboBox.Items.Clear();

            // Add "Default" option
            comboBox.Items.Add(new ComboBoxItem
            {
                Content = "Default (from Sounds tab)",
                Tag = new SoundSelection { Type = SoundType.Default }
            });

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

        private void LoadSoundSettings()
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null) return;

            // Volume
            if (settings.HoldSoundVolume.HasValue)
            {
                HoldVolumeSlider.Value = settings.HoldSoundVolume.Value * 100;
                HoldVolumePercentText.Text = $"{(int)(settings.HoldSoundVolume.Value * 100)}%";
            }
            else
            {
                HoldVolumeSlider.Value = -1;
                HoldVolumePercentText.Text = "Default";
            }

            // Mute sound
            SelectSoundInComboBox(HoldMuteSoundComboBox, settings.HoldMuteSoundPreloaded, settings.HoldMuteSoundCustomPath);

            // Unmute sound
            SelectSoundInComboBox(HoldUnmuteSoundComboBox, settings.HoldUnmuteSoundPreloaded, settings.HoldUnmuteSoundCustomPath);
        }

        private void SelectSoundInComboBox(ComboBox comboBox, string? preloadedKey, string? customPath)
        {
            // If both are null, select "Default"
            if (string.IsNullOrEmpty(preloadedKey) && string.IsNullOrEmpty(customPath))
            {
                comboBox.SelectedIndex = 0; // Default option
                return;
            }

            // Custom path
            if (!string.IsNullOrEmpty(customPath))
            {
                foreach (ComboBoxItem item in comboBox.Items)
                {
                    if (item.Tag is SoundSelection sel && sel.Type == SoundType.Custom && sel.Path == customPath)
                    {
                        comboBox.SelectedItem = item;
                        return;
                    }
                }
            }

            // Preloaded sound
            if (!string.IsNullOrEmpty(preloadedKey))
            {
                foreach (ComboBoxItem item in comboBox.Items)
                {
                    if (item.Tag is SoundSelection sel && sel.Type == SoundType.Preloaded && sel.Key == preloadedKey)
                    {
                        comboBox.SelectedItem = item;
                        return;
                    }
                }
            }

            // Default to "Default" option
            comboBox.SelectedIndex = 0;
        }

        private void UpdateActionDescription(string action)
        {
            ActionDescriptionText.Text = action switch
            {
                "Toggle" => "Toggle between muted/unmuted while holding",
                "HoldToMute" => "Mute while holding (unmute on release)",
                "HoldToUnmute" => "Unmute while holding (mute on release)",
                _ => "Toggle between muted/unmuted while holding"
            };
        }

        #region Event Handlers

        private void RecordHoldHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            if (_isRecordingHoldHotkey)
            {
                StopRecordingHoldHotkey();
            }
            else
            {
                StartRecordingHoldHotkey();
            }
        }

        private void ClearHoldHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            HoldHotkeyTextBox.Text = "";
            App.Instance?.SettingsService.UpdateHoldHotkey(0, ModifierKeys.None);
            App.Instance?.KeyboardHookService.UpdateHoldHotkey(0, ModifierKeys.None, IgnoreHoldModifiersCheckBox.IsChecked ?? true);
        }

        private void IgnoreHoldModifiersCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            var ignore = IgnoreHoldModifiersCheckBox.IsChecked ?? true;
            App.Instance?.SettingsService.UpdateIgnoreHoldModifiers(ignore);
            
            var settings = App.Instance?.SettingsService.Settings;
            if (settings != null)
            {
                App.Instance?.KeyboardHookService.UpdateHoldHotkey(
                    settings.HoldHotkeyCode,
                    settings.HoldHotkeyModifiers,
                    ignore);
            }
        }

        private void HoldPlaySoundsCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            if (_isInitializing) return;
            
            var playSounds = HoldPlaySoundsCheckBox.IsChecked ?? true;
            App.Instance?.SettingsService.UpdateHoldPlaySounds(playSounds);
        }

        private void HoldShowOverlayCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            if (_isInitializing) return;
            
            var showOverlay = HoldShowOverlayCheckBox.IsChecked ?? true;
            App.Instance?.SettingsService.UpdateHoldShowOverlay(showOverlay);
        }

        private void HoldActionComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (_isInitializing) return;
            
            if (HoldActionComboBox.SelectedItem is ComboBoxItem item)
            {
                var action = item.Tag?.ToString() ?? "Toggle";
                App.Instance?.SettingsService.UpdateHoldAction(action);
                UpdateActionDescription(action);
            }
        }

        private void HoldVolumeSlider_ValueChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e)
        {
            if (HoldVolumePercentText == null) return;

            if (HoldVolumeSlider.Value < 0)
            {
                HoldVolumePercentText.Text = "Default";
            }
            else
            {
                var volumePercent = (int)HoldVolumeSlider.Value;
                HoldVolumePercentText.Text = $"{volumePercent}%";
            }

            if (_isInitializing) return;

            if (HoldVolumeSlider.Value < 0)
            {
                App.Instance?.SettingsService.UpdateHoldSoundVolume(-1); // Will be stored as null
            }
            else
            {
                var volume = (float)(HoldVolumeSlider.Value / 100.0);
                App.Instance?.SettingsService.UpdateHoldSoundVolume(volume);
            }
        }

        private void ResetHoldVolumeButton_Click(object sender, RoutedEventArgs e)
        {
            _isInitializing = true;
            HoldVolumeSlider.Value = -1;
            HoldVolumePercentText.Text = "Default";
            _isInitializing = false;
            App.Instance?.SettingsService.UpdateHoldSoundVolume(-1);
        }

        private async void HoldMuteSoundComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (_isInitializing) return;
            if (HoldMuteSoundComboBox.SelectedItem is not ComboBoxItem item) return;
            if (item.Tag is not SoundSelection selection) return;

            if (selection.Type == SoundType.Browse)
            {
                await BrowseForSoundFile(true);
                return;
            }

            if (selection.Type == SoundType.Default)
            {
                App.Instance?.SettingsService.UpdateHoldMuteSound(null, null);
            }
            else if (selection.Type == SoundType.Preloaded)
            {
                App.Instance?.SettingsService.UpdateHoldMuteSound(selection.Key, null);
            }
            else if (selection.Type == SoundType.Custom && selection.Path != null)
            {
                App.Instance?.SettingsService.UpdateHoldMuteSound(null, selection.Path);
            }
        }

        private async void HoldUnmuteSoundComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (_isInitializing) return;
            if (HoldUnmuteSoundComboBox.SelectedItem is not ComboBoxItem item) return;
            if (item.Tag is not SoundSelection selection) return;

            if (selection.Type == SoundType.Browse)
            {
                await BrowseForSoundFile(false);
                return;
            }

            if (selection.Type == SoundType.Default)
            {
                App.Instance?.SettingsService.UpdateHoldUnmuteSound(null, null);
            }
            else if (selection.Type == SoundType.Preloaded)
            {
                App.Instance?.SettingsService.UpdateHoldUnmuteSound(selection.Key, null);
            }
            else if (selection.Type == SoundType.Custom && selection.Path != null)
            {
                App.Instance?.SettingsService.UpdateHoldUnmuteSound(null, selection.Path);
            }
        }

        private void ResetHoldMuteSoundButton_Click(object sender, RoutedEventArgs e)
        {
            _isInitializing = true;
            HoldMuteSoundComboBox.SelectedIndex = 0; // Default
            _isInitializing = false;
            App.Instance?.SettingsService.UpdateHoldMuteSound(null, null);
        }

        private void ResetHoldUnmuteSoundButton_Click(object sender, RoutedEventArgs e)
        {
            _isInitializing = true;
            HoldUnmuteSoundComboBox.SelectedIndex = 0; // Default
            _isInitializing = false;
            App.Instance?.SettingsService.UpdateHoldUnmuteSound(null, null);
        }

        private void PlayHoldMuteSoundButton_Click(object sender, RoutedEventArgs e)
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null) return;

            // Get hold-specific sound or fall back to default
            var preloadedKey = settings.HoldMuteSoundPreloaded ?? settings.MuteSoundPreloaded;
            var customPath = settings.HoldMuteSoundCustomPath ?? settings.MuteSoundCustomPath;
            var volume = settings.HoldSoundVolume ?? settings.SoundVolume;

            var path = App.Instance?.SoundService?.GetSoundPath(preloadedKey, customPath, true);
            App.Instance?.SoundService?.PlaySound(path, volume);
        }

        private void PlayHoldUnmuteSoundButton_Click(object sender, RoutedEventArgs e)
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null) return;

            // Get hold-specific sound or fall back to default
            var preloadedKey = settings.HoldUnmuteSoundPreloaded ?? settings.UnmuteSoundPreloaded;
            var customPath = settings.HoldUnmuteSoundCustomPath ?? settings.UnmuteSoundCustomPath;
            var volume = settings.HoldSoundVolume ?? settings.SoundVolume;

            var path = App.Instance?.SoundService?.GetSoundPath(preloadedKey, customPath, false);
            App.Instance?.SoundService?.PlaySound(path, volume);
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
                    // Refresh comboboxes
                    PopulateSoundComboBoxes();

                    // Set as current sound
                    if (isMute)
                    {
                        App.Instance?.SettingsService.UpdateHoldMuteSound(null, addedPath);
                        SelectSoundInComboBox(HoldMuteSoundComboBox, null, addedPath);
                    }
                    else
                    {
                        App.Instance?.SettingsService.UpdateHoldUnmuteSound(null, addedPath);
                        SelectSoundInComboBox(HoldUnmuteSoundComboBox, null, addedPath);
                    }

                    _isInitializing = false;
                    return;
                }
            }

            // Revert selection if cancelled or failed
            var settings = App.Instance?.SettingsService.Settings;
            if (isMute)
            {
                SelectSoundInComboBox(HoldMuteSoundComboBox, settings?.HoldMuteSoundPreloaded, settings?.HoldMuteSoundCustomPath);
            }
            else
            {
                SelectSoundInComboBox(HoldUnmuteSoundComboBox, settings?.HoldUnmuteSoundPreloaded, settings?.HoldUnmuteSoundCustomPath);
            }

            _isInitializing = false;
        }

        #endregion

        #region Recording

        private void StartRecordingHoldHotkey()
        {
            _isRecordingHoldHotkey = true;
            _recordedKeyCode = 0;
            _recordedModifiers = ModifierKeys.None;
            RecordHoldHotkeyButton.Content = "Cancel";
            HoldHotkeyTextBox.Text = "Press keys...";
            HoldHotkeyHintText.Visibility = Visibility.Visible;
            
            // Focus the TextBox to prevent Space from clicking the button
            HoldHotkeyTextBox.Focus(FocusState.Programmatic);
            
            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.ResetRecordingState();
                App.Instance.KeyboardHookService.IsRecording = true;
            }
        }

        private void StopRecordingHoldHotkey()
        {
            _isRecordingHoldHotkey = false;
            RecordHoldHotkeyButton.Content = "Record";
            HoldHotkeyHintText.Visibility = Visibility.Collapsed;
            HoldHotkeyProgressBar.Visibility = Visibility.Collapsed;
            
            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.IsRecording = false;
                App.Instance.KeyboardHookService.ResetRecordingState();
            }
        }

        private void OnModifiersChanged(ModifierKeys modifiers)
        {
            if (!_isRecordingHoldHotkey) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                _recordedModifiers = modifiers;
                var display = VirtualKeys.GetHotkeyDisplayString(0, modifiers);
                var text = string.IsNullOrEmpty(display) ? "Press keys..." : display + " + ...";
                
                HoldHotkeyTextBox.Text = text;
            });
        }

        private void OnModifierHoldProgress(double progress)
        {
            if (!_isRecordingHoldHotkey) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                HoldHotkeyProgressBar.Value = progress;
                HoldHotkeyProgressBar.Visibility = progress > 0 ? Visibility.Visible : Visibility.Collapsed;
            });
        }

        private void OnKeyPressed(int keyCode, ModifierKeys modifiers)
        {
            if (!_isRecordingHoldHotkey) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                _recordedKeyCode = keyCode;
                _recordedModifiers = modifiers;

                var displayText = VirtualKeys.GetHotkeyDisplayString(keyCode, modifiers);
                
                HoldHotkeyTextBox.Text = displayText;
                StopRecordingHoldHotkey();

                App.Instance?.SettingsService.UpdateHoldHotkey(keyCode, modifiers);
                App.Instance?.KeyboardHookService.UpdateHoldHotkey(
                    keyCode,
                    modifiers,
                    IgnoreHoldModifiersCheckBox.IsChecked ?? true);
            });
        }

        #endregion
    }
}
