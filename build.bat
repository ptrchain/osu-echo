@echo off
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0"
title osu-echo build

set "CARGO_FEATURES="
set "COPY_DIR="
set "RUN_TESTS="
set "PAUSE_AT_END=1"

:parse
if "%~1"=="" goto parsed
if /i "%~1"=="-h" goto usage
if /i "%~1"=="--help" goto usage
if /i "%~1"=="--no-pause" (set "PAUSE_AT_END=" & shift & goto parse)
if /i "%~1"=="--test" (set "RUN_TESTS=1" & shift & goto parse)
if /i "%~1"=="--legacy-tls" (set "CARGO_FEATURES=--features legacy-tls" & shift & goto parse)
if /i "%~1"=="--features" (
    if "%~2"=="" (echo [X] --features needs a feature name, e.g. --features legacy-tls & goto fail)
    set "CARGO_FEATURES=--features %~2"
    shift
    shift
    goto parse
)
if /i "%~1"=="--copy" (
    if "%~2"=="" (echo [X] --copy needs a destination folder, e.g. --copy "E:\Code\osuserver\game" & goto fail)
    set "COPY_DIR=%~2"
    shift
    shift
    goto parse
)
echo [X] Unknown option: %~1
goto usage

:parsed
where cargo >nul 2>&1
if errorlevel 1 (
    echo.
    echo [X] cargo was not found on PATH.
    echo     Install Rust from https://rustup.rs ^(Rust must be the default host^),
    echo     then open a new terminal and run this script again.
    goto fail
)

echo.
echo === osu-echo build ===
echo   folder  : %CD%
if defined CARGO_FEATURES (echo   features: %CARGO_FEATURES%) else (echo   features: none)
if defined RUN_TESTS (echo   tests   : yes) else (echo   tests   : no)
if defined COPY_DIR (echo   copy to : !COPY_DIR!) else (echo   copy to : none)
echo.

if defined CARGO_FEATURES (
    rem Vendored OpenSSL builds itself with perl and needs ExtUtils::MakeMaker, which the perl
    rem bundled with Git for Windows does not ship. Failing here beats waiting several minutes
    rem for cargo to discover it the hard way.
    call :check_perl
    if errorlevel 1 (
        echo [X] The in-process legacy TLS path compiles a vendored OpenSSL, which needs a perl
        echo     that has ExtUtils::MakeMaker. No perl on this machine can do that ^(see above^).
        echo     Install a full perl ^(ActivePerl or Strawberry Perl^), or build without the
        echo     feature: the default build still accepts --legacy-tls and uses the python
        echo     front-end instead, which needs nothing but Python 3.
        goto fail
    )
)

if defined RUN_TESTS (
    echo --- cargo test --release ---
    cargo test --release %CARGO_FEATURES%
    if errorlevel 1 goto fail
    echo.
)

echo --- cargo build --release ---
cargo build --release %CARGO_FEATURES%
if errorlevel 1 goto fail

set "EXE=%CD%\target\release\osu-echo.exe"
if not exist "%EXE%" (
    echo.
    echo [X] cargo reported success but "%EXE%" does not exist.
    goto fail
)

echo.
echo [OK] %EXE%
for %%F in ("%EXE%") do echo     %%~zF bytes

if defined COPY_DIR (
    if not exist "!COPY_DIR!\" (
        echo [X] Destination folder does not exist: !COPY_DIR!
        goto fail
    )
    echo.
    echo --- copying to !COPY_DIR! ---
    rem No pipes in these one-liners: the whole -Command argument is quoted, so cmd already hands
    rem the pipes through untouched and a caret escape would reach PowerShell as a literal '^'.
    powershell -NoProfile -ExecutionPolicy Bypass -Command ^
        "$target = [IO.Path]::GetFullPath((Join-Path '!COPY_DIR!' 'osu-echo.exe')); foreach ($p in @(Get-Process osu-echo -ErrorAction SilentlyContinue)) { if ($p.Path -eq $target) { Write-Host ('     stopping the server running from that folder (pid ' + $p.Id + ')'); Stop-Process -Id $p.Id -Force } }; Start-Sleep -Milliseconds 500"
    rem A --legacy-tls front-end is a child python process; a hard kill of the server leaves it
    rem behind still holding port 443, which would break the next start with 'port in use'.
    powershell -NoProfile -ExecutionPolicy Bypass -Command ^
        "foreach ($p in @(Get-CimInstance -ClassName Win32_Process)) { if ($p.Name -like 'python*' -and $p.CommandLine -like '*legacy_tls_proxy.py*') { Write-Host ('     stopping a leftover legacy TLS front-end (pid ' + $p.ProcessId + ')'); Stop-Process -Id $p.ProcessId -Force } }"
    copy /y "%EXE%" "!COPY_DIR!\osu-echo.exe" >nul
    if errorlevel 1 (
        echo [X] Could not replace !COPY_DIR!\osu-echo.exe - is it still running?
        goto fail
    )
    echo [OK] !COPY_DIR!\osu-echo.exe
    echo     start it with: "!COPY_DIR!\osu-echo.exe" --legacy-tls
)

echo.
echo Run the tests with:      build.bat --test
echo Legacy clients need:     build.bat --legacy-tls
echo Build and install with:  build.bat --test --copy "E:\Code\osuserver\game"
if "%PAUSE_AT_END%"=="1" pause
exit /b 0

:check_perl
rem Same perl cargo's build would use: PATH first, then the one Git for Windows ships.
set "_perl="
for /f "delims=" %%P in ('where perl 2^>nul') do if not defined _perl set "_perl=%%P"
if not defined _perl if exist "%ProgramFiles%\Git\usr\bin\perl.exe" set "_perl=%ProgramFiles%\Git\usr\bin\perl.exe"
if not defined _perl if exist "%ProgramFiles(x86)%\Git\usr\bin\perl.exe" set "_perl=%ProgramFiles(x86)%\Git\usr\bin\perl.exe"
if not defined _perl (
    echo [X] No perl found, and building the vendored OpenSSL needs one.
    exit /b 1
)
"!_perl!" -MExtUtils::MakeMaker -e 1 >nul 2>&1
if errorlevel 1 (
    echo [X] "!_perl!" cannot build the vendored OpenSSL: it has no ExtUtils::MakeMaker.
    exit /b 1
)
exit /b 0

:usage
echo.
echo Usage: build.bat [OPTIONS]
echo.
echo   --test            run the test suite before building
echo   --legacy-tls      also build the in-process OpenSSL TLS path (needs perl)
echo   --features NAME   pass a Cargo feature list, e.g. --features legacy-tls
echo   --copy DIR        copy the built exe into DIR, e.g. "E:\Code\osuserver\game"
echo   --no-pause        do not pause when finished
echo.
echo Examples:
echo   build.bat --test --copy "E:\Code\osuserver\game"
echo   build.bat --legacy-tls
if "%PAUSE_AT_END%"=="1" pause
exit /b 0

:fail
echo.
echo Build failed.
if "%PAUSE_AT_END%"=="1" pause
exit /b 1
