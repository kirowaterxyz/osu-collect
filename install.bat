@echo off
setlocal enableextensions enabledelayedexpansion

:: install.bat -- installs or updates osu-collect from the latest GitHub release
:: requires: Windows 10+ (PowerShell 5.1+ and curl.exe built-in, both preinstalled)

set "REPO=uwuclxdy/osu-collect"
set "API_URL=https://api.github.com/repos/%REPO%/releases/latest"
set "ASSET_NAME=osu-collect-windows-x64.exe"
set "INSTALL_DIR=%LOCALAPPDATA%\Programs\osu-collect"
set "INSTALL_BIN=%INSTALL_DIR%\osu-collect.exe"
set "TMPDIR=%TEMP%\osu-collect-install-%RANDOM%"

:: -- fetch latest release tag via PowerShell ----------------------------------

echo ==^> fetching latest release info...

for /f "delims=" %%T in ('powershell -NoProfile -Command ^
  "$r = Invoke-RestMethod -Uri '%API_URL%' -UseBasicParsing; $r.tag_name"') do (
  set "LATEST_TAG=%%T"
)
if "%LATEST_TAG%"=="" (
  echo error: could not fetch latest release tag
  exit /b 1
)

:: -- build asset URLs ---------------------------------------------------------

for /f "delims=" %%U in ('powershell -NoProfile -Command ^
  "$r = Invoke-RestMethod -Uri '%API_URL%' -UseBasicParsing; ($r.assets | Where-Object { $_.name -eq '%ASSET_NAME%' }).browser_download_url"') do (
  set "DOWNLOAD_URL=%%U"
)
if "%DOWNLOAD_URL%"=="" (
  echo error: asset '%ASSET_NAME%' not found in release %LATEST_TAG%
  exit /b 1
)

for /f "delims=" %%U in ('powershell -NoProfile -Command ^
  "$r = Invoke-RestMethod -Uri '%API_URL%' -UseBasicParsing; ($r.assets | Where-Object { $_.name -eq '%ASSET_NAME%.sha256' }).browser_download_url"') do (
  set "SHA256_URL=%%U"
)
if "%SHA256_URL%"=="" (
  echo error: checksum file '%ASSET_NAME%.sha256' not found in release %LATEST_TAG%
  exit /b 1
)

echo ==^> latest release: %LATEST_TAG%

:: -- create temp dir ----------------------------------------------------------

mkdir "%TMPDIR%" 2>nul
if errorlevel 1 (
  echo error: could not create temp directory %TMPDIR%
  exit /b 1
)

set "TMP_BIN=%TMPDIR%\%ASSET_NAME%"
set "TMP_SHA=%TMPDIR%\%ASSET_NAME%.sha256"

:: -- download checksum first --------------------------------------------------

echo ==^> downloading checksum...
curl.exe -fsSL --retry 3 -o "%TMP_SHA%" "%SHA256_URL%"
if errorlevel 1 (
  echo error: failed to download checksum file
  rmdir /s /q "%TMPDIR%" 2>nul
  exit /b 1
)

:: extract remote hash (first token on first line)
for /f "usebackq tokens=1" %%H in ("!TMP_SHA!") do (
  set "REMOTE_HASH=%%H"
  goto :got_remote_hash
)
:got_remote_hash
if "%REMOTE_HASH%"=="" (
  echo error: could not read hash from checksum file
  rmdir /s /q "%TMPDIR%" 2>nul
  exit /b 1
)

:: -- idempotency check --------------------------------------------------------

set "CURRENT_HASH="
if exist "%INSTALL_BIN%" (
  for /f "delims=" %%H in ('powershell -NoProfile -Command ^
    "(Get-FileHash \"%INSTALL_BIN%\" -Algorithm SHA256).Hash.ToLower()"') do (
    set "CURRENT_HASH=%%H"
  )
)

if defined CURRENT_HASH (
  set "REMOTE_LOWER="
  for /f "delims=" %%H in ('powershell -NoProfile -Command ^
    "'%REMOTE_HASH%'.ToLower()"') do set "REMOTE_LOWER=%%H"

  if "!CURRENT_HASH!"=="!REMOTE_LOWER!" (
    echo ==^> already up to date (%LATEST_TAG%^)
    rmdir /s /q "%TMPDIR%" 2>nul
    goto :shortcuts
  )
)

:: -- download binary ----------------------------------------------------------

echo ==^> downloading osu-collect %LATEST_TAG%...
curl.exe -fsSL --retry 3 -o "%TMP_BIN%" "%DOWNLOAD_URL%"
if errorlevel 1 (
  echo error: download failed
  rmdir /s /q "%TMPDIR%" 2>nul
  exit /b 1
)

:: -- verify sha256 ------------------------------------------------------------

echo ==^> verifying checksum...
for /f "delims=" %%H in ('powershell -NoProfile -Command ^
  "(Get-FileHash \"%TMP_BIN%\" -Algorithm SHA256).Hash.ToLower()"') do (
  set "ACTUAL_HASH=%%H"
)

set "REMOTE_LOWER="
for /f "delims=" %%H in ('powershell -NoProfile -Command ^
  "'%REMOTE_HASH%'.ToLower()"') do set "REMOTE_LOWER=%%H"

if not "!ACTUAL_HASH!"=="!REMOTE_LOWER!" (
  echo error: sha256 mismatch
  echo   expected: !REMOTE_LOWER!
  echo   actual:   !ACTUAL_HASH!
  del /f /q "%TMP_BIN%" 2>nul
  rmdir /s /q "%TMPDIR%" 2>nul
  exit /b 1
)

:: -- install binary -----------------------------------------------------------

if not exist "%INSTALL_DIR%" (
  mkdir "%INSTALL_DIR%"
  if errorlevel 1 (
    echo error: could not create install directory %INSTALL_DIR%
    rmdir /s /q "%TMPDIR%" 2>nul
    exit /b 1
  )
)

copy /y "%TMP_BIN%" "%INSTALL_BIN%" >nul
if errorlevel 1 (
  echo error: could not copy binary to %INSTALL_BIN%
  rmdir /s /q "%TMPDIR%" 2>nul
  exit /b 1
)
echo ==^> installed to %INSTALL_BIN%

:: -- add to user PATH if needed -----------------------------------------------

:: read current user PATH from registry (not the process PATH) to avoid duplicates
for /f "tokens=2*" %%A in (
  'reg query "HKCU\Environment" /v Path 2^>nul'
) do set "USER_PATH=%%B"

:: check if install dir is already in user path
echo !USER_PATH! | findstr /i /c:"%INSTALL_DIR%" >nul 2>&1
if errorlevel 1 (
  if defined USER_PATH (
    setx PATH "!USER_PATH!;%INSTALL_DIR%" >nul
  ) else (
    setx PATH "%INSTALL_DIR%" >nul
  )
  if errorlevel 1 (
    echo error: could not update user PATH
  ) else (
    echo ==^> added %INSTALL_DIR% to user PATH
    echo     restart your terminal for PATH changes to take effect
  )
) else (
  echo ==^> %INSTALL_DIR% already in user PATH
)

:: -- shortcuts ----------------------------------------------------------------

:shortcuts
echo ==^> creating shortcuts...
:: if Windows Terminal is present, launch through it so the shortcut carries the
:: Windows Terminal icon and opens in wt instead of the legacy console
powershell -NoProfile -Command ^
  "$ws = New-Object -ComObject WScript.Shell; " ^
  "$q = [char]34; " ^
  "$bang = [char]33; " ^
  "$name = 'osu' + $bang + 'collect.lnk'; " ^
  "$wt = [System.IO.Path]::Combine($env:LOCALAPPDATA, 'Microsoft\WindowsApps\wt.exe'); " ^
  "$useWt = Test-Path $wt; " ^
  "$paths = @( " ^
  "  [System.Environment]::GetFolderPath('Desktop'), " ^
  "  [System.IO.Path]::Combine($env:APPDATA, 'Microsoft\Windows\Start Menu\Programs') " ^
  "); " ^
  "foreach ($dir in $paths) { " ^
  "  $lnk = $ws.CreateShortcut([System.IO.Path]::Combine($dir, $name)); " ^
  "  if ($useWt) { " ^
  "    $lnk.TargetPath = $wt; " ^
  "    $lnk.Arguments = $q + '%INSTALL_BIN%' + $q; " ^
  "  } else { " ^
  "    $lnk.TargetPath = '%INSTALL_BIN%'; " ^
  "  } " ^
  "  $lnk.WorkingDirectory = $env:USERPROFILE; " ^
  "  $lnk.Description = 'download osu' + $bang + ' collections (TUI)'; " ^
  "  $lnk.Save() " ^
  "}"
if errorlevel 1 (
  echo error: could not create shortcuts
) else (
  echo ==^> shortcuts created in Desktop and Start Menu
)

:: -- register uninstaller -----------------------------------------------------

:: extract the uninstaller payload (plain PowerShell at the bottom of this file)
:: into the install dir, then register it under HKCU so it appears in the Windows
:: "Installed apps" list with a working Uninstall button
echo ==^> registering uninstaller...
powershell -NoProfile -Command ^
  "$dir = '%INSTALL_DIR%'; " ^
  "$u = [System.IO.Path]::Combine($dir, 'uninstall.ps1'); " ^
  "$marker = '# ::OSU-COLLECT' + '-UNINSTALLER::'; " ^
  "$txt = [IO.File]::ReadAllText('%~f0'); " ^
  "$i = $txt.IndexOf($marker); " ^
  "if ($i -ge 0) { [IO.File]::WriteAllText($u, $txt.Substring($i + $marker.Length).TrimStart()); } " ^
  "$q = [char]34; " ^
  "$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\osu-collect'; " ^
  "New-Item -Path $key -Force | Out-Null; " ^
  "$us = 'powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File ' + $q + $u + $q; " ^
  "Set-ItemProperty -Path $key -Name DisplayName -Value ('osu' + [char]33 + 'collect'); " ^
  "Set-ItemProperty -Path $key -Name DisplayIcon -Value '%INSTALL_BIN%'; " ^
  "Set-ItemProperty -Path $key -Name DisplayVersion -Value ('%LATEST_TAG%'.TrimStart('v')); " ^
  "Set-ItemProperty -Path $key -Name Publisher -Value 'uwuclxdy'; " ^
  "Set-ItemProperty -Path $key -Name InstallLocation -Value $dir; " ^
  "Set-ItemProperty -Path $key -Name URLInfoAbout -Value 'https://github.com/uwuclxdy/osu-collect'; " ^
  "Set-ItemProperty -Path $key -Name UninstallString -Value $us; " ^
  "Set-ItemProperty -Path $key -Name QuietUninstallString -Value $us; " ^
  "New-ItemProperty -Path $key -Name NoModify -Value 1 -PropertyType DWord -Force | Out-Null; " ^
  "New-ItemProperty -Path $key -Name NoRepair -Value 1 -PropertyType DWord -Force | Out-Null"
if errorlevel 1 (
  echo error: could not register uninstaller
) else (
  echo ==^> uninstaller registered; see Settings - Apps - Installed apps
)

:: -- cleanup ------------------------------------------------------------------

rmdir /s /q "%TMPDIR%" 2>nul
echo ==^> done -- open a new terminal and run 'osu-collect' to start
endlocal
exit /b 0

:: Everything below runs only when extracted to uninstall.ps1 (PowerShell), never
:: by cmd -- it sits past `exit /b 0`. The installer copies it verbatim into the
:: install dir and registers it as the Windows uninstaller.
# ::OSU-COLLECT-UNINSTALLER::
$ErrorActionPreference = 'SilentlyContinue'
$installDir = Join-Path $env:LOCALAPPDATA 'Programs\osu-collect'
$desktop    = [Environment]::GetFolderPath('Desktop')
$startMenu  = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$shortcuts  = @(
    Join-Path $desktop   'osu!collect.lnk'
    Join-Path $startMenu 'osu!collect.lnk'
    Join-Path $desktop   'osu-collect.lnk'
    Join-Path $startMenu 'osu-collect.lnk'
)
foreach ($lnk in $shortcuts) { Remove-Item -LiteralPath $lnk -Force }

# strip the install dir from the user PATH
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath) {
    $kept = ($userPath -split ';') | Where-Object { $_ -and ($_ -ne $installDir) }
    [Environment]::SetEnvironmentVariable('Path', ($kept -join ';'), 'User')
}

# drop the "Installed apps" registry entry
Remove-Item -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\osu-collect' -Recurse -Force

# remove the install dir (incl. this script) from a detached process so the file
# isn't locked while it deletes itself
Start-Process cmd -WindowStyle Hidden -ArgumentList '/c', ('timeout /t 2 > nul & rmdir /s /q "' + $installDir + '"')
