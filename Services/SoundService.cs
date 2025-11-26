using NAudio.Wave;
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading.Tasks;

namespace silence_.Services;

/// <summary>
/// Service for managing and playing mute/unmute sounds
/// </summary>
public class SoundService : IDisposable
{
    private readonly string _customSoundsFolder;
    private readonly string _preloadedSoundsFolder;
    private IWavePlayer? _waveOut;
    private AudioFileReader? _audioFile;
    private readonly object _playLock = new();
    
    // Preloaded sound names
    public static readonly List<PreloadedSound> PreloadedSounds = new()
    {
        new PreloadedSound("None", null),
        new PreloadedSound("8-Bit", "8bit"),
        new PreloadedSound("Blob", "blob"),
        new PreloadedSound("Digital", "digital"),
        new PreloadedSound("Discord", "discord"),
        new PreloadedSound("Pop", "pop"),
        new PreloadedSound("Punchy", "punchy"),
        new PreloadedSound("Sci-Fi", "scifi"),
        new PreloadedSound("Vibrant", "vibrant"),
    };
    
    public SoundService()
    {
        // Custom sounds folder in AppData
        _customSoundsFolder = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "silence",
            "sounds");
        
        Directory.CreateDirectory(_customSoundsFolder);
        
        // Preloaded sounds folder
        _preloadedSoundsFolder = Path.Combine(AppContext.BaseDirectory, "Assets", "sounds");
    }
    
    public string CustomSoundsFolder => _customSoundsFolder;
    
    public string PreloadedSoundsFolder => _preloadedSoundsFolder;
    
    /// <summary>
    /// Gets list of custom sound files added by user
    /// </summary>
    public List<CustomSound> GetCustomSounds()
    {
        var sounds = new List<CustomSound>();
        
        try
        {
            if (Directory.Exists(_customSoundsFolder))
            {
                var files = Directory.GetFiles(_customSoundsFolder, "*.*")
                    .Where(f => IsSupportedAudioFile(f));
                    
                foreach (var file in files)
                {
                    sounds.Add(new CustomSound(Path.GetFileNameWithoutExtension(file), file));
                }
            }
        }
        catch
        {
            // Ignore errors reading custom sounds folder
        }
        
        return sounds;
    }
    
    /// <summary>
    /// Copies a sound file to the custom sounds folder
    /// </summary>
    public async Task<string?> AddCustomSoundAsync(string sourcePath)
    {
        try
        {
            var fileName = Path.GetFileName(sourcePath);
            var destPath = Path.Combine(_customSoundsFolder, fileName);
            
            // If file already exists, add a number suffix
            var counter = 1;
            var nameWithoutExt = Path.GetFileNameWithoutExtension(fileName);
            var ext = Path.GetExtension(fileName);
            while (File.Exists(destPath))
            {
                destPath = Path.Combine(_customSoundsFolder, $"{nameWithoutExt}_{counter}{ext}");
                counter++;
            }
            
            await Task.Run(() => File.Copy(sourcePath, destPath));
            return destPath;
        }
        catch
        {
            return null;
        }
    }
    
    /// <summary>
    /// Deletes a custom sound file
    /// </summary>
    public bool DeleteCustomSound(string filePath)
    {
        try
        {
            if (File.Exists(filePath) && filePath.StartsWith(_customSoundsFolder))
            {
                File.Delete(filePath);
                return true;
            }
        }
        catch
        {
            // Ignore deletion errors
        }
        
        return false;
    }
    
    /// <summary>
    /// Gets the full path for a sound based on settings
    /// </summary>
    public string? GetSoundPath(string? preloadedKey, string? customPath, bool isMute)
    {
        // Custom path takes precedence
        if (!string.IsNullOrEmpty(customPath) && File.Exists(customPath))
        {
            return customPath;
        }
        
        // Try preloaded sound
        if (!string.IsNullOrEmpty(preloadedKey))
        {
            var suffix = isMute ? "_mute" : "_unmute";
            var soundFile = Path.Combine(_preloadedSoundsFolder, $"{preloadedKey}{suffix}.mp3");
            
            if (File.Exists(soundFile))
            {
                return soundFile;
            }
        }
        
        return null;
    }
    
    /// <summary>
    /// Plays a sound file (fire and forget, no media controls integration)
    /// </summary>
    /// <param name="filePath">Path to audio file</param>
    /// <param name="volume">Volume level 0.0 - 1.0</param>
    public void PlaySound(string? filePath, float volume = 0.5f)
    {
        if (string.IsNullOrEmpty(filePath) || !File.Exists(filePath))
            return;
            
        // Play in background to not block UI
        var vol = Math.Clamp(volume, 0f, 1f);
        Task.Run(() => PlaySoundInternal(filePath, vol));
    }
    
    private void PlaySoundInternal(string filePath, float volume)
    {
        lock (_playLock)
        {
            try
            {
                // Stop and dispose previous playback
                StopCurrentPlayback();
                
                _audioFile = new AudioFileReader(filePath);
                _audioFile.Volume = volume;
                _waveOut = new WasapiOut();
                _waveOut.Init(_audioFile);
                _waveOut.PlaybackStopped += OnPlaybackStopped;
                _waveOut.Play();
            }
            catch
            {
                // Ignore playback errors - sound files might be corrupted or unsupported
                StopCurrentPlayback();
            }
        }
    }
    
    private void OnPlaybackStopped(object? sender, StoppedEventArgs e)
    {
        // Cleanup after playback finishes
        Task.Run(() =>
        {
            lock (_playLock)
            {
                StopCurrentPlayback();
            }
        });
    }
    
    private void StopCurrentPlayback()
    {
        try
        {
            if (_waveOut != null)
            {
                _waveOut.PlaybackStopped -= OnPlaybackStopped;
                _waveOut.Stop();
                _waveOut.Dispose();
                _waveOut = null;
            }
            
            _audioFile?.Dispose();
            _audioFile = null;
        }
        catch
        {
            // Ignore cleanup errors
        }
    }
    
    /// <summary>
    /// Plays the mute sound based on settings
    /// </summary>
    public void PlayMuteSound(AppSettings settings)
    {
        if (!settings.SoundsEnabled)
            return;
            
        var path = GetSoundPath(settings.MuteSoundPreloaded, settings.MuteSoundCustomPath, true);
        PlaySound(path, settings.SoundVolume);
    }
    
    /// <summary>
    /// Plays the unmute sound based on settings
    /// </summary>
    public void PlayUnmuteSound(AppSettings settings)
    {
        if (!settings.SoundsEnabled)
            return;
            
        var path = GetSoundPath(settings.UnmuteSoundPreloaded, settings.UnmuteSoundCustomPath, false);
        PlaySound(path, settings.SoundVolume);
    }
    
    private static bool IsSupportedAudioFile(string path)
    {
        var ext = Path.GetExtension(path).ToLowerInvariant();
        return ext is ".mp3" or ".wav" or ".wma" or ".m4a" or ".flac" or ".ogg";
    }
    
    public void Dispose()
    {
        lock (_playLock)
        {
            StopCurrentPlayback();
        }
    }
}

public record PreloadedSound(string DisplayName, string? Key);
public record CustomSound(string DisplayName, string FilePath);

