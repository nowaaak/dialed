@echo off
rem Startet die Release-Einrichtung (Repo, Secrets, erstes Release).
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0setup-release.ps1"
pause
