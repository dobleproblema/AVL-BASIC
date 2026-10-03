@echo off
setlocal
rem Right-click this file and choose Run as administrator. No automatic elevation.
set "AVL_PMU_PS=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
if exist "%SystemRoot%\Sysnative\WindowsPowerShell\v1.0\powershell.exe" set "AVL_PMU_PS=%SystemRoot%\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
"%AVL_PMU_PS%" -NoProfile -ExecutionPolicy Bypass -File "%~dp0capture_cross_cpu_pmu.ps1" -PackageRoot "%~dp0." -Profiles IPC -Pilot %*
set "AVL_PMU_EXIT=%ERRORLEVEL%"
echo.
if not "%AVL_PMU_EXIT%"=="0" echo PMU capture did not complete. Review the diagnostic path shown above.
pause
exit /b %AVL_PMU_EXIT%
