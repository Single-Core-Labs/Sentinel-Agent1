@echo off
rem web-guard (Windows) dispatcher: delegates to guard.ps1.
rem Contract: invoked as `guard <event> <tool_name>`, full event JSON on stdin.
if /i not "%~2"=="web_fetch" if /i not "%~2"=="web_search" (
  echo allow
  exit /b 0
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0guard.ps1"
exit /b 0
