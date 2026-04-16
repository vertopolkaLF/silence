using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using silence_.Services;
using System;
using System.Collections.Generic;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace silence_.Pages
{
    public sealed partial class HoldToMutePage : Page
    {
        private string? _recordingBindingId;
        private bool _isInitializing;
        private readonly Dictionary<string, HoldHotkeyRowControls> _holdHotkeyRows = new();

        public HoldToMutePage()
        {
            InitializeComponent();
            ApplyLocalizedStrings();
            LoadSettings();

            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.KeyPressed += OnKeyPressed;
                App.Instance.KeyboardHookService.ModifiersChanged += OnModifiersChanged;
                App.Instance.KeyboardHookService.ModifierHoldProgress += OnModifierHoldProgress;
            }

            if (App.Instance?.GamepadInputService != null)
            {
                App.Instance.GamepadInputService.ButtonsCaptured += OnGamepadButtonsCaptured;
                App.Instance.GamepadInputService.RecordingButtonsChanged += OnGamepadButtonsChanged;
                App.Instance.GamepadInputService.ButtonHoldProgress += OnGamepadHoldProgress;
            }
        }

        private void ApplyLocalizedStrings()
        {
            TitleTextBlock.Text = AppResources.GetString("HoldToMutePage.TitleText.Text");
            DescriptionTextBlock.Text = AppResources.GetString("HoldToMutePage.DescriptionText.Text");
            HotkeyLabelText.Text = AppResources.GetString("HoldToMutePage.HotkeyLabel.Text");
            AddHoldHotkeyButton.Content = AppResources.GetString("GeneralPage.AddHotkeyButton.Content");
            IgnoreHoldModifiersCheckBox.Content = AppResources.GetString("HoldToMutePage.IgnoreHoldModifiersCheckBox.Content");
            ActionLabelText.Text = AppResources.GetString("HoldToMutePage.ActionLabel.Text");
            ActionToggleItem.Content = AppResources.GetString("HoldToMutePage.ActionToggleItem.Content");
            ActionHoldToMuteItem.Content = AppResources.GetString("HoldToMutePage.ActionHoldToMuteItem.Content");
            ActionHoldToUnmuteItem.Content = AppResources.GetString("HoldToMutePage.ActionHoldToUnmuteItem.Content");
            OptionsLabelText.Text = AppResources.GetString("HoldToMutePage.OptionsLabel.Text");
            HoldPlaySoundsCheckBox.Content = AppResources.GetString("HoldToMutePage.HoldPlaySoundsCheckBox.Content");
            HoldShowOverlayCheckBox.Content = AppResources.GetString("HoldToMutePage.HoldShowOverlayCheckBox.Content");
            SoundSettingsTitleText.Text = AppResources.GetString("HoldToMutePage.SoundSettingsTitle.Text");
            SoundSettingsDescriptionText.Text = AppResources.GetString("HoldToMutePage.SoundSettingsDescription.Text");
            VolumeLabelText.Text = AppResources.GetString("HoldToMutePage.VolumeLabel.Text");
            ResetHoldVolumeButton.Content = AppResources.GetString("HoldToMutePage.ResetHoldVolumeButton.Content");
            MuteSoundLabelText.Text = AppResources.GetString("HoldToMutePage.MuteSoundLabel.Text");
            HoldMuteSoundComboBox.PlaceholderText = AppResources.GetString("HoldToMutePage.HoldMuteSoundComboBox.PlaceholderText");
            ToolTipService.SetToolTip(PlayHoldMuteSoundButton, AppResources.GetString("HoldToMutePage.PlayHoldMuteSoundButton.ToolTipService.ToolTip"));
            ResetHoldMuteSoundButton.Content = AppResources.GetString("HoldToMutePage.ResetHoldMuteSoundButton.Content");
            UnmuteSoundLabelText.Text = AppResources.GetString("HoldToMutePage.UnmuteSoundLabel.Text");
            HoldUnmuteSoundComboBox.PlaceholderText = AppResources.GetString("HoldToMutePage.HoldUnmuteSoundComboBox.PlaceholderText");
            ToolTipService.SetToolTip(PlayHoldUnmuteSoundButton, AppResources.GetString("HoldToMutePage.PlayHoldUnmuteSoundButton.ToolTipService.ToolTip"));
            ResetHoldUnmuteSoundButton.Content = AppResources.GetString("HoldToMutePage.ResetHoldUnmuteSoundButton.Content");
        }

        private void LoadSettings()
        {
            _isInitializing = true;

            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null)
            {
                _isInitializing = false;
                return;
            }

            ReloadHoldHotkeyRows();
            IgnoreHoldModifiersCheckBox.IsChecked = settings.IgnoreHoldModifiers;
            HoldPlaySoundsCheckBox.IsChecked = settings.HoldPlaySounds;
            HoldShowOverlayCheckBox.IsChecked = settings.HoldShowOverlay;

            HoldActionComboBox.SelectedIndex = settings.HoldAction switch
            {
                "Toggle" => 0,
                "HoldToMute" => 1,
                "HoldToUnmute" => 2,
                _ => 0
            };
            UpdateActionDescription(settings.HoldAction);

            PopulateSoundComboBoxes();
            LoadSoundSettings();

            _isInitializing = false;
        }

        private void ReloadHoldHotkeyRows()
        {
            if (_recordingBindingId != null)
            {
                StopRecordingHoldHotkey();
            }

            _holdHotkeyRows.Clear();
            HoldHotkeysPanel.Children.Clear();

            var bindings = App.Instance?.SettingsService.GetHoldHotkeyBindings();
            if (bindings == null)
            {
                return;
            }

            foreach (var binding in bindings)
            {
                var row = CreateHoldHotkeyRow(binding);
                _holdHotkeyRows[binding.Id] = row;
                HoldHotkeysPanel.Children.Add(row.Container);
            }
        }

        private HoldHotkeyRowControls CreateHoldHotkeyRow(HoldHotkeyBindingSettings binding)
        {
            var container = new StackPanel
            {
                Spacing = 6
            };

            var grid = new Grid
            {
                ColumnSpacing = 8
            };
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

            var textHost = new Grid();
            var textBox = new TextBox
            {
                IsReadOnly = true,
                PlaceholderText = AppResources.GetString("HoldToMutePage.HoldHotkeyTextBox.PlaceholderText"),
                Text = GetDisplayText(binding)
            };
            var progressBar = new ProgressBar
            {
                Height = 3,
                VerticalAlignment = VerticalAlignment.Bottom,
                Margin = new Thickness(1, 0, 1, 1),
                Minimum = 0,
                Maximum = 1,
                Value = 0,
                Visibility = Visibility.Collapsed,
                ShowError = false,
                ShowPaused = false
            };
            textHost.Children.Add(textBox);
            textHost.Children.Add(progressBar);
            Grid.SetColumn(textHost, 0);
            grid.Children.Add(textHost);

            var clearButton = new Button
            {
                Tag = binding.Id,
                Content = "\uE711",
                FontFamily = Application.Current.Resources["SymbolThemeFontFamily"] as FontFamily,
                Width = 32,
                Height = 32,
                Padding = new Thickness(0)
            };
            clearButton.Click += ClearHoldHotkeyButton_Click;
            ToolTipService.SetToolTip(clearButton, AppResources.GetString("HoldToMutePage.ClearHoldHotkeyButton.ToolTipService.ToolTip"));
            Grid.SetColumn(clearButton, 1);
            grid.Children.Add(clearButton);

            var recordButton = new Button
            {
                Tag = binding.Id,
                Content = AppResources.GetString("HoldToMutePage.RecordHoldHotkeyButton.Content")
            };
            recordButton.Click += RecordHoldHotkeyButton_Click;
            Grid.SetColumn(recordButton, 2);
            grid.Children.Add(recordButton);

            var removeButton = new Button
            {
                Tag = binding.Id,
                Content = "\uE74D",
                FontFamily = Application.Current.Resources["SymbolThemeFontFamily"] as FontFamily,
                Width = 32,
                Height = 32,
                Padding = new Thickness(0)
            };
            removeButton.Click += RemoveHoldHotkeyButton_Click;
            ToolTipService.SetToolTip(removeButton, AppResources.GetString("GeneralPage.RemoveHotkeyButton.ToolTipService.ToolTip"));
            Grid.SetColumn(removeButton, 3);
            grid.Children.Add(removeButton);

            var hintText = new TextBlock
            {
                Text = AppResources.GetString("HoldToMutePage.HoldHotkeyHintText.Text"),
                FontSize = 11,
                Opacity = 0.6,
                Visibility = Visibility.Collapsed,
                Margin = new Thickness(0, 4, 0, 0)
            };

            container.Children.Add(grid);
            container.Children.Add(hintText);

            return new HoldHotkeyRowControls(binding.Id, container, textBox, progressBar, hintText, recordButton);
        }

        private void RefreshHoldHotkeyRegistration()
        {
            var holdBindings = App.Instance?.SettingsService.GetHoldHotkeyBindings();
            var ignoreModifiers = App.Instance?.SettingsService.Settings.IgnoreHoldModifiers ?? true;
            App.Instance?.KeyboardHookService.UpdateHoldHotkeys(holdBindings, ignoreModifiers);
            App.Instance?.GamepadInputService.UpdateHoldHotkeys(holdBindings);
        }

        private static string GetDisplayText(HoldHotkeyBindingSettings binding)
        {
            return InputBindingDisplay.GetDisplayText(binding);
        }

        private void UpdateActionDescription(string action)
        {
            ActionDescriptionText.Text = action switch
            {
                "Toggle" => AppResources.GetString("HoldToMute.ActionDescription.Toggle"),
                "HoldToMute" => AppResources.GetString("HoldToMute.ActionDescription.HoldToMute"),
                "HoldToUnmute" => AppResources.GetString("HoldToMute.ActionDescription.HoldToUnmute"),
                _ => AppResources.GetString("HoldToMute.ActionDescription.Toggle")
            };
        }

        #region Event Handlers

        private void AddHoldHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            App.Instance?.SettingsService.AddHoldHotkeyBinding();
            ReloadHoldHotkeyRows();
            RefreshHoldHotkeyRegistration();
        }

        private void RecordHoldHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            if (sender is not Button button || button.Tag is not string bindingId)
            {
                return;
            }

            if (_recordingBindingId == bindingId)
            {
                StopRecordingHoldHotkey();
            }
            else
            {
                StartRecordingHoldHotkey(bindingId);
            }
        }

        private void ClearHoldHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            if (sender is not Button button || button.Tag is not string bindingId)
            {
                return;
            }

            if (_recordingBindingId == bindingId)
            {
                StopRecordingHoldHotkey();
            }

            App.Instance?.SettingsService.UpdateHoldHotkeyBinding(bindingId, 0, ModifierKeys.None);
            ReloadHoldHotkeyRows();
            RefreshHoldHotkeyRegistration();
        }

        private void RemoveHoldHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            if (sender is not Button button || button.Tag is not string bindingId)
            {
                return;
            }

            if (_recordingBindingId == bindingId)
            {
                StopRecordingHoldHotkey();
            }

            App.Instance?.SettingsService.RemoveHoldHotkeyBinding(bindingId);
            ReloadHoldHotkeyRows();
            RefreshHoldHotkeyRegistration();
        }

        private void IgnoreHoldModifiersCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            var ignore = IgnoreHoldModifiersCheckBox.IsChecked ?? true;
            App.Instance?.SettingsService.UpdateIgnoreHoldModifiers(ignore);
            RefreshHoldHotkeyRegistration();
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
                HoldVolumePercentText.Text = AppResources.GetString("Common.Default");
            }
            else
            {
                HoldVolumePercentText.Text = $"{(int)HoldVolumeSlider.Value}%";
            }

            if (_isInitializing) return;

            if (HoldVolumeSlider.Value < 0)
            {
                App.Instance?.SettingsService.UpdateHoldSoundVolume(-1);
            }
            else
            {
                App.Instance?.SettingsService.UpdateHoldSoundVolume((float)(HoldVolumeSlider.Value / 100.0));
            }
        }

        private void ResetHoldVolumeButton_Click(object sender, RoutedEventArgs e)
        {
            _isInitializing = true;
            HoldVolumeSlider.Value = -1;
            HoldVolumePercentText.Text = AppResources.GetString("Common.Default");
            _isInitializing = false;
            App.Instance?.SettingsService.UpdateHoldSoundVolume(-1);
        }

        #endregion

        #region Sounds

        private void PopulateSoundComboBoxes()
        {
            PopulateSoundComboBox(HoldMuteSoundComboBox);
            PopulateSoundComboBox(HoldUnmuteSoundComboBox);
        }

        private void PopulateSoundComboBox(ComboBox comboBox)
        {
            comboBox.Items.Clear();

            comboBox.Items.Add(new ComboBoxItem
            {
                Content = AppResources.GetString("Sounds.DefaultFromSoundsTab"),
                Tag = new SoundSelection { Type = SoundType.Default }
            });

            foreach (var sound in SoundService.PreloadedSounds)
            {
                comboBox.Items.Add(new ComboBoxItem
                {
                    Content = sound.DisplayName,
                    Tag = new SoundSelection { Type = SoundType.Preloaded, Key = sound.Key }
                });
            }

            var customSounds = App.Instance?.SoundService?.GetCustomSounds();
            if (customSounds?.Count > 0)
            {
                comboBox.Items.Add(new ComboBoxItem
                {
                    Content = AppResources.GetString("Sounds.ComboSeparator"),
                    IsEnabled = false,
                    Tag = null
                });

                foreach (var sound in customSounds)
                {
                    comboBox.Items.Add(new ComboBoxItem
                    {
                        Content = AppResources.Format("Sounds.CustomItem", sound.DisplayName),
                        Tag = new SoundSelection { Type = SoundType.Custom, Path = sound.FilePath }
                    });
                }
            }

            comboBox.Items.Add(new ComboBoxItem
            {
                Content = AppResources.GetString("Sounds.ComboSeparator"),
                IsEnabled = false,
                Tag = null
            });
            comboBox.Items.Add(new ComboBoxItem
            {
                Content = AppResources.GetString("Sounds.BrowseForFile"),
                Tag = new SoundSelection { Type = SoundType.Browse }
            });
        }

        private void LoadSoundSettings()
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null) return;

            if (settings.HoldSoundVolume.HasValue)
            {
                HoldVolumeSlider.Value = settings.HoldSoundVolume.Value * 100;
                HoldVolumePercentText.Text = $"{(int)(settings.HoldSoundVolume.Value * 100)}%";
            }
            else
            {
                HoldVolumeSlider.Value = -1;
                HoldVolumePercentText.Text = AppResources.GetString("Common.Default");
            }

            SelectSoundInComboBox(HoldMuteSoundComboBox, settings.HoldMuteSoundPreloaded, settings.HoldMuteSoundCustomPath);
            SelectSoundInComboBox(HoldUnmuteSoundComboBox, settings.HoldUnmuteSoundPreloaded, settings.HoldUnmuteSoundCustomPath);
        }

        private void SelectSoundInComboBox(ComboBox comboBox, string? preloadedKey, string? customPath)
        {
            if (string.IsNullOrEmpty(preloadedKey) && string.IsNullOrEmpty(customPath))
            {
                comboBox.SelectedIndex = 0;
                return;
            }

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

            comboBox.SelectedIndex = 0;
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
            HoldMuteSoundComboBox.SelectedIndex = 0;
            _isInitializing = false;
            App.Instance?.SettingsService.UpdateHoldMuteSound(null, null);
        }

        private void ResetHoldUnmuteSoundButton_Click(object sender, RoutedEventArgs e)
        {
            _isInitializing = true;
            HoldUnmuteSoundComboBox.SelectedIndex = 0;
            _isInitializing = false;
            App.Instance?.SettingsService.UpdateHoldUnmuteSound(null, null);
        }

        private void PlayHoldMuteSoundButton_Click(object sender, RoutedEventArgs e)
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null) return;

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
                var addedPath = await soundService.AddCustomSoundAsync(file.Path);
                if (addedPath != null)
                {
                    PopulateSoundComboBoxes();

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

        private void StartRecordingHoldHotkey(string bindingId)
        {
            if (_recordingBindingId != null)
            {
                StopRecordingHoldHotkey();
            }

            _recordingBindingId = bindingId;
            var controls = GetHoldHotkeyRow(bindingId);
            controls.RecordButton.Content = AppResources.GetString("HoldToMute.Hotkey.RecordCancel");
            controls.TextBox.Text = AppResources.GetString("Hotkeys.RecordPrompt");
            controls.HintText.Visibility = Visibility.Visible;
            controls.TextBox.Focus(FocusState.Programmatic);

            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.ResetRecordingState();
                App.Instance.KeyboardHookService.IsRecording = true;
            }

            if (App.Instance?.GamepadInputService != null)
            {
                App.Instance.GamepadInputService.ResetRecordingState();
                App.Instance.GamepadInputService.IsRecording = true;
            }
        }

        private void StopRecordingHoldHotkey()
        {
            if (_recordingBindingId != null && _holdHotkeyRows.TryGetValue(_recordingBindingId, out var controls))
            {
                controls.RecordButton.Content = AppResources.GetString("HoldToMute.Hotkey.Record");
                controls.HintText.Visibility = Visibility.Collapsed;
                controls.ProgressBar.Visibility = Visibility.Collapsed;
            }

            _recordingBindingId = null;

            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.IsRecording = false;
                App.Instance.KeyboardHookService.ResetRecordingState();
            }

            if (App.Instance?.GamepadInputService != null)
            {
                App.Instance.GamepadInputService.IsRecording = false;
                App.Instance.GamepadInputService.ResetRecordingState();
            }
        }

        private void OnModifiersChanged(ModifierKeys modifiers)
        {
            if (_recordingBindingId == null) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                var bindingId = _recordingBindingId;
                if (bindingId == null || !_holdHotkeyRows.TryGetValue(bindingId, out var controls))
                {
                    return;
                }

                var display = InputBindingDisplay.GetKeyboardHotkeyDisplayString(
                    0,
                    modifiers,
                    App.Instance?.KeyboardHookService.RecordingChordKeys);
                controls.TextBox.Text = string.IsNullOrEmpty(display)
                    ? AppResources.GetString("Hotkeys.RecordPrompt")
                    : AppResources.Format("Hotkeys.RecordPromptWithModifiers", display);
            });
        }

        private void OnModifierHoldProgress(double progress)
        {
            if (_recordingBindingId == null) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                var bindingId = _recordingBindingId;
                if (bindingId == null || !_holdHotkeyRows.TryGetValue(bindingId, out var controls))
                {
                    return;
                }

                controls.ProgressBar.Value = progress;
                controls.ProgressBar.Visibility = progress > 0 ? Visibility.Visible : Visibility.Collapsed;
            });
        }

        private void OnGamepadHoldProgress(double progress)
        {
            if (_recordingBindingId == null) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                var bindingId = _recordingBindingId;
                if (bindingId == null || !_holdHotkeyRows.TryGetValue(bindingId, out var controls))
                {
                    return;
                }

                controls.ProgressBar.Value = progress;
                controls.ProgressBar.Visibility = progress > 0 ? Visibility.Visible : Visibility.Collapsed;
            });
        }

        private void OnGamepadButtonsChanged(ulong buttonsMask, IReadOnlyList<int> chordKeyCodes)
        {
            if (_recordingBindingId == null) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                var bindingId = _recordingBindingId;
                if (bindingId == null || !_holdHotkeyRows.TryGetValue(bindingId, out var controls))
                {
                    return;
                }

                var display = InputBindingDisplay.GetGamepadButtonsDisplayString(buttonsMask, chordKeyCodes);
                controls.TextBox.Text = string.IsNullOrEmpty(display)
                    ? AppResources.GetString("Hotkeys.RecordPrompt")
                    : AppResources.Format("Hotkeys.RecordPromptWithModifiers", display);
            });
        }

        private void OnKeyPressed(int keyCode, ModifierKeys modifiers, IReadOnlyList<int> chordKeyCodes)
        {
            if (_recordingBindingId == null) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                var bindingId = _recordingBindingId;
                if (bindingId == null)
                {
                    return;
                }

                StopRecordingHoldHotkey();
                App.Instance?.SettingsService.UpdateKeyboardHoldHotkeyBinding(bindingId, keyCode, modifiers, chordKeyCodes);
                ReloadHoldHotkeyRows();
                RefreshHoldHotkeyRegistration();
            });
        }

        private void OnGamepadButtonsCaptured(ulong buttonsMask, IReadOnlyList<int> chordKeyCodes)
        {
            if (_recordingBindingId == null) return;

            DispatcherQueue.TryEnqueue(() =>
            {
                var bindingId = _recordingBindingId;
                if (bindingId == null)
                {
                    return;
                }

                StopRecordingHoldHotkey();
                App.Instance?.SettingsService.UpdateGamepadHoldHotkeyBinding(
                    bindingId,
                    InputBindingDisplay.GetButtonsFromMask(buttonsMask),
                    chordKeyCodes);
                ReloadHoldHotkeyRows();
                RefreshHoldHotkeyRegistration();
            });
        }

        #endregion

        protected override void OnNavigatedFrom(Microsoft.UI.Xaml.Navigation.NavigationEventArgs e)
        {
            base.OnNavigatedFrom(e);
            StopRecordingHoldHotkey();

            if (App.Instance?.KeyboardHookService != null)
            {
                App.Instance.KeyboardHookService.KeyPressed -= OnKeyPressed;
                App.Instance.KeyboardHookService.ModifiersChanged -= OnModifiersChanged;
                App.Instance.KeyboardHookService.ModifierHoldProgress -= OnModifierHoldProgress;
            }

            if (App.Instance?.GamepadInputService != null)
            {
                App.Instance.GamepadInputService.ButtonsCaptured -= OnGamepadButtonsCaptured;
                App.Instance.GamepadInputService.RecordingButtonsChanged -= OnGamepadButtonsChanged;
                App.Instance.GamepadInputService.ButtonHoldProgress -= OnGamepadHoldProgress;
            }
        }

        private HoldHotkeyRowControls GetHoldHotkeyRow(string bindingId)
        {
            if (_holdHotkeyRows.TryGetValue(bindingId, out var row))
            {
                return row;
            }

            throw new InvalidOperationException($"Hold hotkey row '{bindingId}' was not found.");
        }

        private sealed class HoldHotkeyRowControls
        {
            public HoldHotkeyRowControls(
                string bindingId,
                StackPanel container,
                TextBox textBox,
                ProgressBar progressBar,
                TextBlock hintText,
                Button recordButton)
            {
                BindingId = bindingId;
                Container = container;
                TextBox = textBox;
                ProgressBar = progressBar;
                HintText = hintText;
                RecordButton = recordButton;
            }

            public string BindingId { get; }
            public StackPanel Container { get; }
            public TextBox TextBox { get; }
            public ProgressBar ProgressBar { get; }
            public TextBlock HintText { get; }
            public Button RecordButton { get; }
        }
    }
}
