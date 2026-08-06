@echo off
rem =============================================================================
rem clone-all.bat  (in .tmp)
rem
rem Clones every discord-cli repo from the research list into .tmp\, each into
rem its own <owner>-<repo>\ folder. Prints the ".git" URL for each.
rem
rem Usage:
rem   clone-all.bat              - clone all repos
rem   clone-all.bat --dry-run    - print URLs only, no cloning
rem =============================================================================
setlocal enabledelayedexpansion
chcp 65001 >nul
where git >nul 2>nul || ( echo [ERROR] Git not found on PATH. & exit /b 1 )

set "BASE_DIR=%~dp0"
set "DRY=%~1"
set /a COUNT=0

rem owner:repo:url  (url uses .git suffix)
set "R1=jackwener:discord-cli:https://github.com/jackwener/discord-cli.git"
set "R2=fourjr:discord-cli:https://github.com/fourjr/discord-cli.git"
set "R3=Escape-Technologies:discord-cli:https://github.com/Escape-Technologies/discord-cli.git"
set "R4=RickvanLoo:discord-cli:https://github.com/RickvanLoo/discord-cli.git"
set "R5=Rivalo:discord-cli:https://github.com/Rivalo/discord-cli.git"
set "R6=famasya:discord-cli-agent:https://github.com/famasya/discord-cli-agent.git"
set "R7=mrarfarf:discord-cli:https://github.com/mrarfarf/discord-cli.git"
set "R8=langkurt:discord-cli:https://github.com/langkurt/discord-cli.git"
set "R9=virat-mankali:discord-cli:https://github.com/virat-mankali/discord-cli.git"
set "R10=ibbybuilds:discli:https://github.com/ibbybuilds/discli.git"
set "R11=ThePolishCat:discord-cli:https://github.com/ThePolishCat/discord-cli.git"
set "R12=Stone-Red-Code:DiscordCLI:https://github.com/Stone-Red-Code/DiscordCLI.git"
set "R13=ayn2op:discordo:https://github.com/ayn2op/discordo.git"
set "R14=sinjs:clicord:https://github.com/sinjs/clicord.git"

for %%R in (1 2 3 4 5 6 7 8 9 10 11 12 13 14) do (
  call :clone_entry %%R
)
echo.
echo Done. %COUNT% repo^(s^) cloned into %BASE_DIR%
goto :eof

:clone_entry
setlocal
set "ENTRY=!R%1!"
for /f "tokens=1,2,3 delims=:" %%A in ("!ENTRY!") do (
  set "OWNER=%%A"
  set "REPO=%%B"
  set "URL=%%C"
)
set "DIR=!OWNER!-!REPO!"
if "!DRY!"=="--dry-run" (
  echo   !DIR!  ^<-  !URL!
  exit /b 0
)
if exist "!BASE_DIR!!DIR!\.git" (
  echo [skip] !DIR! ^(!URL!^)
) else (
  echo [clone] !DIR! ^<- !URL!
  git clone --quiet "!URL!" "!BASE_DIR!!DIR!"
  if errorlevel 1 (
    echo   [FAIL] !URL!
  ) else (
    set /a COUNT+=1
  )
)
endlocal & set /a COUNT=%COUNT%
exit /b 0
