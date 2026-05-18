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
powershell -NoProfile -Command ^
  "$ws = New-Object -ComObject WScript.Shell; " ^
  "$paths = @( " ^
  "  [System.Environment]::GetFolderPath('Desktop'), " ^
  "  [System.IO.Path]::Combine($env:APPDATA, 'Microsoft\Windows\Start Menu\Programs') " ^
  "); " ^
  "foreach ($dir in $paths) { " ^
  "  $lnk = $ws.CreateShortcut([System.IO.Path]::Combine($dir, 'osu-collect.lnk')); " ^
  "  $lnk.TargetPath = \"%INSTALL_BIN%\"; " ^
  "  $lnk.WorkingDirectory = $env:USERPROFILE; " ^
  "  $lnk.Description = 'download osu! collections (TUI)'; " ^
  "  $lnk.Save() " ^
  "}"
if errorlevel 1 (
  echo error: could not create shortcuts
) else (
  echo ==^> shortcuts created in Desktop and Start Menu
)

:: -- cleanup ------------------------------------------------------------------

rmdir /s /q "%TMPDIR%" 2>nul
echo ==^> done -- open a new terminal and run 'osu-collect' to start
endlocal
exit /b 0
