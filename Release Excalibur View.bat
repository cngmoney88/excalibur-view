@echo off
title Excalibur View - publishing a release
cd /d "%~dp0"

rem Double-click this. It publishes a release: tests, builds, signs it with the
rem key on this computer, and puts it on GitHub. The website and every
rem installed copy pick it up by themselves.
rem
rem Everything it actually does is in tools\release_now.py, so that the part
rem worth reading is readable.

set PY=
for %%p in (py.exe) do if not "%%~$PATH:p"=="" set PY=py -3
if "%PY%"=="" for %%p in (python.exe) do if not "%%~$PATH:p"=="" set PY=python

if "%PY%"=="" (
  echo.
  echo   Python is not installed on this computer, and this needs it.
  echo.
  echo   Get it from python.org/downloads - tick "Add Python to PATH"
  echo   on the first screen of the installer - then double-click this again.
  echo.
  pause
  exit /b 1
)

%PY% tools\release_now.py
if errorlevel 1 pause
