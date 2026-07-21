@echo off
setlocal
cd /d "%~dp0"

if not exist "ai-config.json" (
  if exist "config.example.json" (
    copy /Y "config.example.json" "ai-config.json" >nul
    echo Created ai-config.json from example. Please fill in your API Key.
    echo.
  )
)

if not exist "data" mkdir data

cargo run --release -p aipet-server -- --bind 0.0.0.0:8787 --data-dir "%~dp0data" --ai-config "%~dp0ai-config.json"
endlocal
