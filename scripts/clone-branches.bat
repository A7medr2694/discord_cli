@echo off
rem =============================================================================
rem clone-branches.bat
rem
rem Clones every remote branch of every discord-cli repo already cloned in
rem .tmp\ into .tmp\branches\<owner-repo>\<branch>.
rem
rem Usage:
rem   clone-branches.bat               - clone all branches of all repos
rem =============================================================================
setlocal enabledelayedexpansion
chcp 65001 >nul

where git >nul 2>nul || (
  echo [ERROR] Git not found on PATH.
  exit /b 1
)

set "BASE_DIR=%~dp0.."
set "TARGET=%BASE_DIR%\.tmp\branches"
if not exist "%TARGET%" mkdir "%TARGET%"

set /a COUNT=0

for /d %%R in ("%BASE_DIR%\.tmp\*") do (
  if exist "%%R\.git" (
    echo === %%~nxR ===
    set "URL="
    for /f "delims=" %%U in ('git -C "%%R" config --get remote.origin.url 2^>nul') do set "URL=%%U"
    if defined URL (
      set "HAS=0"
      for /f "tokens=2" %%B in ('git ls-remote --heads "!URL!" 2^>nul') do (
        set "BRANCH=%%B"
        set "BRANCH=!BRANCH:refs/heads/=!"
        if defined BRANCH (
          set "HAS=1"
          set "SAFE=!BRANCH:/=_!"
          set "DEST=%TARGET%\%%~nxR\!SAFE!"
          if exist "!DEST!\.git" (
            echo   [skip]  !BRANCH!
          ) else (
            echo   [clone] !BRANCH!
            git clone --quiet --depth 1 --branch "!BRANCH!" "!URL!" "!DEST!"
            if errorlevel 1 (
              echo   [FAIL]  !BRANCH!
            ) else (
              set /a COUNT+=1
            )
          )
        )
      )
      if "!HAS!"=="0" echo   ^(no remote branches^)
    )
  )
)

echo.
echo Done. %COUNT% branch clone^(s^) in %TARGET%
endlocal
