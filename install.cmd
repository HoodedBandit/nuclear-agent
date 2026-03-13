@echo off
setlocal
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
set "exitcode=%errorlevel%"
if not "%exitcode%"=="0" (
  echo.
  echo Installation failed with exit code %exitcode%.
  if exist "%~dp0install-error.log" (
    echo See "%~dp0install-error.log" for details.
  )
  echo Press any key to close this window.
  pause >nul
)
exit /b %exitcode%
