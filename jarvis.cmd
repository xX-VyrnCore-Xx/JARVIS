@echo off
rem Avvio di JARVIS su Windows: doppio clic oppure "jarvis.cmd [desktop|voice-setup|doctor]".
rem Esegue jarvis.ps1 senza modificare la politica di esecuzione del sistema.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0jarvis.ps1" %*
if errorlevel 1 pause
