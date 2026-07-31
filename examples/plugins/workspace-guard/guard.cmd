@echo off
rem workspace-guard (Windows) dispatcher: delegates to guard.ps1.
rem Contract: invoked as `guard <event> <tool_name>`, full event JSON on stdin.
if /i not "%~2"=="write" if /i not "%~2"=="edit" if /i not "%~2"=="apply_patch" (
  echo allow
  exit /b 0
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0guard.ps1"
exit /b 0
