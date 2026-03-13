using Microsoft.Windows.ApplicationModel.Resources;
using WinAppSdkApplicationLanguages = Microsoft.Windows.Globalization.ApplicationLanguages;
using System;
using System.Globalization;
using System.Linq;

namespace silence_.Services;

public sealed class LocalizationService
{
    public const string SystemLanguage = "system";
    public const string EnglishLanguage = "en-US";
    public const string RussianLanguage = "ru-RU";

    private static readonly string[] SupportedLanguages = [EnglishLanguage, RussianLanguage];

    private ResourceManager? _resourceManager;
    private ResourceMap? _resourceMap;
    private ResourceContext? _resourceContext;

    public event Action? LanguageChanged;

    public LocalizationService(string? requestedLanguage)
    {
        ApplyLanguage(requestedLanguage, notify: false);
    }

    public string RequestedLanguage { get; private set; } = SystemLanguage;

    public string EffectiveLanguage { get; private set; } = EnglishLanguage;

    public string GetString(string resourceId)
    {
        return TryResolveString(resourceId) ?? resourceId;
    }

    public bool TryGetString(string resourceId, out string value)
    {
        var resolvedValue = TryResolveString(resourceId);
        if (!string.IsNullOrEmpty(resolvedValue))
        {
            value = resolvedValue;
            return true;
        }

        value = string.Empty;
        return false;
    }

    private string? TryResolveString(string resourceId)
    {
        if (!EnsureResourcesInitialized())
        {
            return null;
        }

        foreach (var candidateId in GetLookupCandidates(resourceId))
        {
            var value = _resourceMap?.TryGetValue(candidateId, _resourceContext)?.ValueAsString;
            if (!string.IsNullOrEmpty(value))
            {
                return value;
            }

            value = TryGetNestedValue(candidateId);
            if (!string.IsNullOrEmpty(value))
            {
                return value;
            }
        }

        return null;
    }

    public string Format(string resourceId, params object[] arguments)
    {
        return string.Format(CultureInfo.CurrentCulture, GetString(resourceId), arguments);
    }

    public bool ApplyLanguage(string? requestedLanguage, bool notify = true)
    {
        var normalizedRequested = NormalizeRequestedLanguage(requestedLanguage);
        var effectiveLanguage = ResolveAppLanguage(requestedLanguage);

        RequestedLanguage = normalizedRequested;
        EffectiveLanguage = effectiveLanguage;

        TryApplyPrimaryLanguageOverride(normalizedRequested, effectiveLanguage, notify);

        TryUpdateResourceContextLanguage();

        var culture = CultureInfo.GetCultureInfo(effectiveLanguage);
        CultureInfo.DefaultThreadCurrentCulture = culture;
        CultureInfo.DefaultThreadCurrentUICulture = culture;
        CultureInfo.CurrentCulture = culture;
        CultureInfo.CurrentUICulture = culture;

        if (notify)
        {
            LanguageChanged?.Invoke();
        }

        return true;
    }

    public static string NormalizeRequestedLanguage(string? requestedLanguage)
    {
        if (string.IsNullOrWhiteSpace(requestedLanguage) ||
            string.Equals(requestedLanguage, SystemLanguage, StringComparison.OrdinalIgnoreCase))
        {
            return SystemLanguage;
        }

        return ResolveSupportedLanguage(requestedLanguage) ?? SystemLanguage;
    }

    public static string ResolveAppLanguage(string? requestedLanguage)
    {
        return ResolveEffectiveLanguage(NormalizeRequestedLanguage(requestedLanguage));
    }

    private static string ResolveEffectiveLanguage(string requestedLanguage)
    {
        if (!string.Equals(requestedLanguage, SystemLanguage, StringComparison.OrdinalIgnoreCase))
        {
            return requestedLanguage;
        }

        foreach (var language in GetSystemLanguageCandidates())
        {
            var supportedLanguage = ResolveSupportedLanguage(language);
            if (supportedLanguage != null)
            {
                return supportedLanguage;
            }
        }

        return EnglishLanguage;
    }

    private static string? ResolveSupportedLanguage(string languageTag)
    {
        if (string.IsNullOrWhiteSpace(languageTag))
        {
            return null;
        }

        var exactMatch = SupportedLanguages.FirstOrDefault(
            language => string.Equals(language, languageTag, StringComparison.OrdinalIgnoreCase));
        if (exactMatch != null)
        {
            return exactMatch;
        }

        if (languageTag.StartsWith("ru", StringComparison.OrdinalIgnoreCase))
        {
            return RussianLanguage;
        }

        if (languageTag.StartsWith("en", StringComparison.OrdinalIgnoreCase))
        {
            return EnglishLanguage;
        }

        return null;
    }

    private bool EnsureResourcesInitialized()
    {
        if (_resourceContext != null && _resourceMap != null)
        {
            return true;
        }

        try
        {
            _resourceManager ??= CreateResourceManager();
            _resourceMap ??= _resourceManager.MainResourceMap.TryGetSubtree("Resources") ?? _resourceManager.MainResourceMap;
            _resourceContext ??= _resourceManager.CreateResourceContext();
            _resourceContext.QualifierValues[KnownResourceQualifierName.Language] = EffectiveLanguage;
            return true;
        }
        catch
        {
            ResetResources();
            return false;
        }
    }

    private static ResourceManager CreateResourceManager()
    {
        try
        {
            return new ResourceManager("silence!.pri");
        }
        catch
        {
            return new ResourceManager();
        }
    }

    private void TryUpdateResourceContextLanguage()
    {
        if (_resourceContext == null)
        {
            return;
        }

        _resourceContext.QualifierValues[KnownResourceQualifierName.Language] = EffectiveLanguage;
    }

    private void ResetResources()
    {
        _resourceContext = null;
        _resourceMap = null;
        _resourceManager = null;
    }

    private static string[] GetSystemLanguageCandidates()
    {
        return
        [
            CultureInfo.InstalledUICulture.Name,
            CultureInfo.CurrentUICulture.Name,
            .. WinAppSdkApplicationLanguages.Languages
        ];
    }

    private static void TryApplyPrimaryLanguageOverride(string requestedLanguage, string effectiveLanguage, bool notify)
    {
        if (string.Equals(requestedLanguage, SystemLanguage, StringComparison.OrdinalIgnoreCase) && !notify)
        {
            return;
        }

        try
        {
            // WinAppSDK expects a real language tag; for "system" during runtime we apply the resolved system language.
            WinAppSdkApplicationLanguages.PrimaryLanguageOverride = effectiveLanguage;
        }
        catch
        {
            // ResourceContext/CultureInfo remain the fallback path for unpackaged runtime localization.
        }
    }

    private static string[] GetLookupCandidates(string resourceId)
    {
        var normalizedResourceId = resourceId.Replace('.', '/');
        return normalizedResourceId == resourceId
            ? [resourceId]
            : [resourceId, normalizedResourceId];
    }

    private string? TryGetNestedValue(string resourceId)
    {
        if (_resourceMap == null || _resourceContext == null)
        {
            return null;
        }

        var pathSegments = resourceId
            .Split(['/', '.'], StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        if (pathSegments.Length == 0)
        {
            return null;
        }

        var currentMap = _resourceMap;
        for (var index = 0; index < pathSegments.Length - 1; index++)
        {
            currentMap = currentMap.TryGetSubtree(pathSegments[index]);
            if (currentMap == null)
            {
                return null;
            }
        }

        return currentMap.TryGetValue(pathSegments[^1], _resourceContext)?.ValueAsString;
    }
}

public static class AppResources
{
    public static string GetString(string resourceId)
    {
        return App.Instance?.LocalizationService.GetString(resourceId) ?? resourceId;
    }

    public static bool TryGetString(string resourceId, out string value)
    {
        if (App.Instance?.LocalizationService.TryGetString(resourceId, out value) == true)
        {
            return true;
        }

        value = string.Empty;
        return false;
    }

    public static string Format(string resourceId, params object[] arguments)
    {
        return App.Instance?.LocalizationService.Format(resourceId, arguments)
            ?? string.Format(CultureInfo.CurrentCulture, resourceId, arguments);
    }
}
