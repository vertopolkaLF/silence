using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using silence_.Services;
using System;
using System.Collections.Generic;

namespace silence_.Pages
{
    public sealed partial class GeneralPage : Page
    {
        private string? _recordingBindingId;
        private bool _isMuted;
        private bool _isHovering;
        private bool _isFirstMuteUpdate = true;
        private readonly Dictionary<string, HotkeyRowControls> _hotkeyRows = new();

        private static readonly Windows.UI.Color MutedColor = Windows.UI.Color.FromArgb(255, 205, 60, 70);
        private static readonly Windows.UI.Color MutedHoverColor = Windows.UI.Color.FromArgb(255, 160, 40, 50);
        private static readonly Windows.UI.Color UnmutedColor = Windows.UI.Color.FromArgb(255, 40, 167, 69);
        private static readonly Windows.UI.Color UnmutedHoverColor = Windows.UI.Color.FromArgb(255, 30, 130, 55);
        private static readonly TimeSpan AnimationDuration = TimeSpan.FromMilliseconds(200);

        public GeneralPage()
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

            if (App.Instance != null)
            {
                App.Instance.MuteStateChanged += OnMuteStateChanged;
            }

            UpdateMuteState(App.Instance?.MicrophoneService.IsMuted() ?? false);
        }

        private void OnMuteStateChanged(bool isMuted)
        {
            DispatcherQueue.TryEnqueue(() => UpdateMuteState(isMuted));
        }

        private void LoadSettings()
        {
            var settings = App.Instance?.SettingsService.Settings;
            if (settings == null)
            {
                return;
            }

            RefreshMicrophones();
            ReloadHotkeyRows();

            AutoStartCheckBox.IsChecked = App.Instance?.SettingsService.IsAutoStartEnabled() ?? false;
            StartMinimizedCheckBox.IsChecked = settings.StartMinimized;
        }

        private void RefreshMicrophones()
        {
            var microphones = App.Instance?.MicrophoneService.GetMicrophones();
            if (microphones == null) return;

            MicrophoneComboBox.Items.Clear();

            var defaultItem = new ComboBoxItem
            {
                Content = AppResources.GetString("General.Microphone.Default"),
                Tag = (string?)null
            };
            MicrophoneComboBox.Items.Add(defaultItem);

            var allMicsItem = new ComboBoxItem
            {
                Content = AppResources.GetString("General.Microphone.All"),
                Tag = MicrophoneService.ALL_MICROPHONES_ID
            };
            MicrophoneComboBox.Items.Add(allMicsItem);

            int selectedIndex = 0;
            var selectedId = App.Instance?.SettingsService.Settings.SelectedMicrophoneId;

            if (selectedId == MicrophoneService.ALL_MICROPHONES_ID)
            {
                selectedIndex = 1;
            }
            else
            {
                for (int i = 0; i < microphones.Count; i++)
                {
                    var mic = microphones[i];
                    var item = new ComboBoxItem
                    {
                        Content = mic.IsDefault
                            ? AppResources.Format("General.Microphone.NamedDefault", mic.Name)
                            : mic.Name,
                        Tag = mic.Id
                    };
                    MicrophoneComboBox.Items.Add(item);

                    if (mic.Id == selectedId)
                    {
                        selectedIndex = i + 2;
                    }
                }
            }

            MicrophoneComboBox.SelectedIndex = selectedIndex;
        }

        public void UpdateMuteState(bool isMuted)
        {
            var stateChanged = _isMuted != isMuted;
            _isMuted = isMuted;

            if (_isFirstMuteUpdate)
            {
                _isFirstMuteUpdate = false;
                MuteStatusText.Text = GetMuteStatusText(isMuted);
                MuteStatusTextAlt.Text = GetMuteStatusText(!isMuted);
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
        }

        private void AnimateMuteText(bool isMuted)
        {
            var storyboard = new Storyboard();
            var duration = TimeSpan.FromMilliseconds(280);
            var easing = new QuadraticEase { EasingMode = EasingMode.EaseInOut };

            double outDirection = isMuted ? -30 : 30;
            double inStartPos = isMuted ? 30 : -30;

            MuteStatusTextAlt.Text = GetMuteStatusText(isMuted);
            MuteStatusTransformAlt.TranslateY = inStartPos;
            MuteStatusTransformAlt.ScaleX = 0.8;
            MuteStatusTransformAlt.ScaleY = 0.8;

            var slideOut = new DoubleAnimation
            {
                To = outDirection,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(slideOut, MuteStatusTransform);
            Storyboard.SetTargetProperty(slideOut, "TranslateY");
            storyboard.Children.Add(slideOut);

            var fadeOut = new DoubleAnimation
            {
                To = 0,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(fadeOut, MuteStatusText);
            Storyboard.SetTargetProperty(fadeOut, "Opacity");
            storyboard.Children.Add(fadeOut);

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

            var slideIn = new DoubleAnimation
            {
                To = 0,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(slideIn, MuteStatusTransformAlt);
            Storyboard.SetTargetProperty(slideIn, "TranslateY");
            storyboard.Children.Add(slideIn);

            var fadeIn = new DoubleAnimation
            {
                To = 1,
                Duration = new Duration(duration),
                EasingFunction = easing
            };
            Storyboard.SetTarget(fadeIn, MuteStatusTextAlt);
            Storyboard.SetTargetProperty(fadeIn, "Opacity");
            storyboard.Children.Add(fadeIn);

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

            storyboard.Completed += (s, e) =>
            {
                MuteStatusText.Text = GetMuteStatusText(isMuted);
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
            var currentBrush = MuteButton.Background as SolidColorBrush;
            if (currentBrush == null)
            {
                MuteButton.Background = new SolidColorBrush(targetColor);
                return;
            }

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

        #region Event Handlers

        private void MuteButton_Click(object sender, Microsoft.UI.Xaml.Input.TappedRoutedEventArgs e)
        {
            App.Instance?.ToggleMute();
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

        private void MuteButton_PointerPressed(object sender, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs e)
        {
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

        private void AddHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            if (sender is not Button button || button.Tag is not string action)
            {
                return;
            }

            App.Instance?.SettingsService.AddHotkeyBinding(action);
            ReloadHotkeyRows();
            RefreshHotkeyRegistration();
        }

        private void RecordHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            if (sender is not Button button || button.Tag is not string bindingId)
            {
                return;
            }

            if (_recordingBindingId == bindingId)
            {
                StopRecordingHotkey();
            }
            else
            {
                StartRecordingHotkey(bindingId);
            }
        }

        private void ClearHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            if (sender is not Button button || button.Tag is not string bindingId)
            {
                return;
            }

            if (_recordingBindingId == bindingId)
            {
                StopRecordingHotkey();
            }

            App.Instance?.SettingsService.UpdateHotkeyBinding(bindingId, 0, ModifierKeys.None);
            ReloadHotkeyRows();
            RefreshHotkeyRegistration();
        }

        private void RemoveHotkeyButton_Click(object sender, RoutedEventArgs e)
        {
            if (sender is not Button button || button.Tag is not string bindingId)
            {
                return;
            }

            if (_recordingBindingId == bindingId)
            {
                StopRecordingHotkey();
            }

            App.Instance?.SettingsService.RemoveHotkeyBinding(bindingId);
            ReloadHotkeyRows();
            RefreshHotkeyRegistration();
        }

        private void StartRecordingHotkey(string bindingId)
        {
            if (_recordingBindingId != null)
            {
                StopRecordingHotkey();
            }

            _recordingBindingId = bindingId;
            var controls = GetHotkeyRow(bindingId);
            controls.RecordButton.Content = AppResources.GetString("General.Hotkey.RecordCancel");
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

        private void StopRecordingHotkey()
        {
            if (_recordingBindingId != null && _hotkeyRows.TryGetValue(_recordingBindingId, out var controls))
            {
                controls.RecordButton.Content = AppResources.GetString("General.Hotkey.Record");
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
            if (_recordingBindingId == null)
            {
                return;
            }

            DispatcherQueue.TryEnqueue(() =>
            {
                var bindingId = _recordingBindingId;
                if (bindingId == null || !_hotkeyRows.TryGetValue(bindingId, out var controls))
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
            if (_recordingBindingId == null)
            {
                return;
            }

            DispatcherQueue.TryEnqueue(() =>
            {
                var bindingId = _recordingBindingId;
                if (bindingId == null || !_hotkeyRows.TryGetValue(bindingId, out var controls))
                {
                    return;
                }

                controls.ProgressBar.Value = progress;
                controls.ProgressBar.Visibility = progress > 0 ? Visibility.Visible : Visibility.Collapsed;
            });
        }

        private void OnGamepadHoldProgress(double progress)
        {
            if (_recordingBindingId == null)
            {
                return;
            }

            DispatcherQueue.TryEnqueue(() =>
            {
                var bindingId = _recordingBindingId;
                if (bindingId == null || !_hotkeyRows.TryGetValue(bindingId, out var controls))
                {
                    return;
                }

                controls.ProgressBar.Value = progress;
                controls.ProgressBar.Visibility = progress > 0 ? Visibility.Visible : Visibility.Collapsed;
            });
        }

        private void OnGamepadButtonsChanged(ulong buttonsMask, IReadOnlyList<int> chordKeyCodes)
        {
            if (_recordingBindingId == null)
            {
                return;
            }

            DispatcherQueue.TryEnqueue(() =>
            {
                var bindingId = _recordingBindingId;
                if (bindingId == null || !_hotkeyRows.TryGetValue(bindingId, out var controls))
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
            if (_recordingBindingId == null)
            {
                return;
            }

            DispatcherQueue.TryEnqueue(() =>
            {
                var bindingId = _recordingBindingId;
                if (bindingId == null || !_hotkeyRows.TryGetValue(bindingId, out var controls))
                {
                    return;
                }

                var ignoreModifiers = controls.IgnoreCheckBox.IsChecked ?? true;

                StopRecordingHotkey();

                App.Instance?.SettingsService.UpdateKeyboardHotkeyBinding(bindingId, keyCode, modifiers, chordKeyCodes);
                App.Instance?.SettingsService.UpdateHotkeyBindingIgnoreModifiers(bindingId, ignoreModifiers);
                ReloadHotkeyRows();
                RefreshHotkeyRegistration();
            });
        }

        private void OnGamepadButtonsCaptured(ulong buttonsMask, IReadOnlyList<int> chordKeyCodes)
        {
            if (_recordingBindingId == null)
            {
                return;
            }

            DispatcherQueue.TryEnqueue(() =>
            {
                var bindingId = _recordingBindingId;
                if (bindingId == null)
                {
                    return;
                }

                StopRecordingHotkey();
                App.Instance?.SettingsService.UpdateGamepadHotkeyBinding(
                    bindingId,
                    InputBindingDisplay.GetButtonsFromMask(buttonsMask),
                    chordKeyCodes);
                ReloadHotkeyRows();
                RefreshHotkeyRegistration();
            });
        }

        private void IgnoreModifiersCheckBox_Changed(object sender, RoutedEventArgs e)
        {
            if (sender is not CheckBox checkBox || checkBox.Tag is not string bindingId)
            {
                return;
            }

            var ignore = checkBox.IsChecked ?? true;
            App.Instance?.SettingsService.UpdateHotkeyBindingIgnoreModifiers(bindingId, ignore);
            RefreshHotkeyRegistration();
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

        #endregion

        protected override void OnNavigatedFrom(Microsoft.UI.Xaml.Navigation.NavigationEventArgs e)
        {
            base.OnNavigatedFrom(e);
            StopRecordingHotkey();

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

            if (App.Instance != null)
            {
                App.Instance.MuteStateChanged -= OnMuteStateChanged;
            }
        }

        private void ApplyLocalizedStrings()
        {
            MicrophoneStatusPrefixText.Text = AppResources.GetString("GeneralPage.MicrophoneStatusPrefixText.Text");
            MicrophoneLabelText.Text = AppResources.GetString("GeneralPage.MicrophoneLabel.Text");
            MicrophoneComboBox.PlaceholderText = AppResources.GetString("GeneralPage.MicrophoneComboBox.PlaceholderText");

            HotkeysLabelText.Text = AppResources.GetString("GeneralPage.HotkeysLabel.Text");
            ToggleHotkeyLabelText.Text = AppResources.GetString("GeneralPage.ToggleHotkeyLabel.Text");
            MuteHotkeyLabelText.Text = AppResources.GetString("GeneralPage.MuteHotkeyLabel.Text");
            UnmuteHotkeyLabelText.Text = AppResources.GetString("GeneralPage.UnmuteHotkeyLabel.Text");

            AddToggleHotkeyButton.Content = AppResources.GetString("GeneralPage.AddHotkeyButton.Content");
            AddMuteHotkeyButton.Content = AppResources.GetString("GeneralPage.AddHotkeyButton.Content");
            AddUnmuteHotkeyButton.Content = AppResources.GetString("GeneralPage.AddHotkeyButton.Content");

            StartupLabelText.Text = AppResources.GetString("GeneralPage.StartupLabel.Text");
            AutoStartCheckBox.Content = AppResources.GetString("GeneralPage.AutoStartCheckBox.Content");
            StartMinimizedCheckBox.Content = AppResources.GetString("GeneralPage.StartMinimizedCheckBox.Content");
        }

        private void ReloadHotkeyRows()
        {
            if (_recordingBindingId != null)
            {
                StopRecordingHotkey();
            }

            _hotkeyRows.Clear();
            ToggleHotkeysPanel.Children.Clear();
            MuteHotkeysPanel.Children.Clear();
            UnmuteHotkeysPanel.Children.Clear();

            BuildHotkeyRowsForAction(HotkeyActions.Toggle);
            BuildHotkeyRowsForAction(HotkeyActions.Mute);
            BuildHotkeyRowsForAction(HotkeyActions.Unmute);
        }

        private void BuildHotkeyRowsForAction(string action)
        {
            var bindings = App.Instance?.SettingsService.GetHotkeyBindings(action);
            if (bindings == null)
            {
                return;
            }

            var host = GetHotkeyHost(action);
            foreach (var binding in bindings)
            {
                var row = CreateHotkeyRow(binding);
                _hotkeyRows[binding.Id] = row;
                host.Children.Add(row.Container);
            }
        }

        private HotkeyRowControls CreateHotkeyRow(HotkeyBindingSettings binding)
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
                PlaceholderText = AppResources.GetString("GeneralPage.HotkeyTextBox.PlaceholderText"),
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
            clearButton.Click += ClearHotkeyButton_Click;
            ToolTipService.SetToolTip(clearButton, AppResources.GetString("GeneralPage.ClearHotkeyButton.ToolTipService.ToolTip"));
            Grid.SetColumn(clearButton, 1);
            grid.Children.Add(clearButton);

            var recordButton = new Button
            {
                Tag = binding.Id,
                Content = AppResources.GetString("GeneralPage.RecordHotkeyButton.Content")
            };
            recordButton.Click += RecordHotkeyButton_Click;
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
            removeButton.Click += RemoveHotkeyButton_Click;
            ToolTipService.SetToolTip(removeButton, AppResources.GetString("GeneralPage.RemoveHotkeyButton.ToolTipService.ToolTip"));
            Grid.SetColumn(removeButton, 3);
            grid.Children.Add(removeButton);

            var hintText = new TextBlock
            {
                Text = AppResources.GetString("GeneralPage.HotkeyHintText.Text"),
                FontSize = 11,
                Opacity = 0.6,
                Visibility = Visibility.Collapsed,
                Margin = new Thickness(0, 4, 0, 0)
            };

            var ignoreCheckBox = new CheckBox
            {
                Tag = binding.Id,
                Content = AppResources.GetString("GeneralPage.IgnoreModifiersCheckBox.Content"),
                IsChecked = binding.IgnoreModifiers,
                Visibility = binding.DeviceKind == InputDeviceKind.Gamepad ? Visibility.Collapsed : Visibility.Visible
            };
            ignoreCheckBox.Checked += IgnoreModifiersCheckBox_Changed;
            ignoreCheckBox.Unchecked += IgnoreModifiersCheckBox_Changed;

            container.Children.Add(grid);
            container.Children.Add(hintText);
            container.Children.Add(ignoreCheckBox);

            return new HotkeyRowControls(binding.Id, binding.Action, container, textBox, progressBar, hintText, recordButton, clearButton, removeButton, ignoreCheckBox);
        }

        private void RefreshHotkeyRegistration()
        {
            var bindings = App.Instance?.SettingsService.GetHotkeyBindings();
            if (bindings == null)
            {
                return;
            }

            App.Instance?.KeyboardHookService.UpdateHotkeys(bindings);
            App.Instance?.GamepadInputService.UpdateHotkeys(bindings);
        }

        private HotkeyRowControls GetHotkeyRow(string bindingId)
        {
            if (_hotkeyRows.TryGetValue(bindingId, out var row))
            {
                return row;
            }

            throw new InvalidOperationException($"Hotkey row '{bindingId}' was not found.");
        }

        private StackPanel GetHotkeyHost(string action)
        {
            return action switch
            {
                HotkeyActions.Toggle => ToggleHotkeysPanel,
                HotkeyActions.Mute => MuteHotkeysPanel,
                HotkeyActions.Unmute => UnmuteHotkeysPanel,
                _ => throw new ArgumentOutOfRangeException(nameof(action), action, null)
            };
        }

        private static string GetMuteStatusText(bool isMuted)
        {
            return AppResources.GetString(isMuted ? "General.MuteStatus.Muted" : "General.MuteStatus.Unmuted");
        }

        private static string GetDisplayText(HotkeyBindingSettings binding)
        {
            return InputBindingDisplay.GetDisplayText(binding);
        }

        private sealed class HotkeyRowControls
        {
            public HotkeyRowControls(
                string bindingId,
                string action,
                StackPanel container,
                TextBox textBox,
                ProgressBar progressBar,
                TextBlock hintText,
                Button recordButton,
                Button clearButton,
                Button removeButton,
                CheckBox ignoreCheckBox)
            {
                BindingId = bindingId;
                Action = action;
                Container = container;
                TextBox = textBox;
                ProgressBar = progressBar;
                HintText = hintText;
                RecordButton = recordButton;
                ClearButton = clearButton;
                RemoveButton = removeButton;
                IgnoreCheckBox = ignoreCheckBox;
            }

            public string BindingId { get; }
            public string Action { get; }
            public StackPanel Container { get; }
            public TextBox TextBox { get; }
            public ProgressBar ProgressBar { get; }
            public TextBlock HintText { get; }
            public Button RecordButton { get; }
            public Button ClearButton { get; }
            public Button RemoveButton { get; }
            public CheckBox IgnoreCheckBox { get; }
        }
    }
}
