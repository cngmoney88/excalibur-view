@echo off
title Excalibur View - handing the release to every office
cd /d "%~dp0"

rem Double-click this after a release has been tried on one seat and was fine.
rem It takes the newest release off the early channel and gives it to every
rem office, which is the moment it becomes the version everybody runs.

set PY=
for %%p in (py.exe) do if not "%%~$PATH:p"=="" set PY=py -3
if "%PY%"=="" for %%p in (python.exe) do if not "%%~$PATH:p"=="" set PY=python

if "%PY%"=="" (
  echo.
  echo   Python is not installed on this computer, and this needs it.
  echo.
  pause
  exit /b 1
)

%PY% tools\release_now.py --promote
if errorlevel 1 pause
