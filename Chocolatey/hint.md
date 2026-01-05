1. Пересобрать инсталлеры
Тебе нужно пересобрать инсталлеры с новыми настройками:
cd C:\Dev\Silence!

# Запусти свой build скрипт
.\build-installers.ps1
cd C:\Dev\Silence!# Запусти свой build скрипт.\build-installers.ps1
2. Создать новый GitHub Release
После сборки инсталлеров:
Создай новый release (например v1.4.1 или перезалей v1.4)
Загрузи новые инсталлеры
3. Обновить Chocolatey пакет
cd C:\Users\leo20\silence!

# Пересобрать с новыми чексуммами
.\build-chocolatey.ps1 -Version "1.4"

# Протестировать
choco install silence -s . -y

# Если всё ок - удалить
choco uninstall silence -y
\build-chocolatey.ps1 -Version "1.4"

# Протестировать
choco install silence -s . -y

# Если всё ок - удалить
choco uninstall silence -y

4. Опубликовать
choco push silence.1.4.0.nupkg --source https://push.chocolatey.org/