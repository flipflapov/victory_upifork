@echo off
setlocal
rem Windows PowerShell by its full path: a PATH without its folder left
rem "powershell.exe" unknown. PowerShell 7 (pwsh) if that is missing.
set "PS=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
if not exist "%PS%" set "PS=pwsh.exe"
"%PS%" -NoLogo -NoProfile -STA -ExecutionPolicy Bypass -File "%~dp0Setup-UpiLauncher.ps1" %*
if errorlevel 1 (
    echo.
    echo Setup stopped. See the message above.
    pause
    exit /b 1
)
