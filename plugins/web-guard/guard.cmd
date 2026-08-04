@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0guard.ps1" %1 %2
