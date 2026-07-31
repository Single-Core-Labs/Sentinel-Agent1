@echo off
rem command-guard (Windows) dispatcher: delegates to guard.ps1.
rem Contract: invoked as `guard <event> <tool_name>`, full event JSON on stdin.
if /i not "%~2"=="run_shell_command" (
  echo allow
  exit /b 0
)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0guard.ps1"
exit /b 0
