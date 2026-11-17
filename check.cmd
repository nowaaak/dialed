@echo off
rem Prueft, ob das Projekt unter Windows kompiliert, und schreibt das Ergebnis nach build-log.txt.
cd /d "%~dp0"
echo Dialed Build-Check laeuft, bitte warten ...
echo ==== Dialed Build-Check ==== > build-log.txt
cargo --version >> build-log.txt 2>&1
cargo test -p dialed-core >> build-log.txt 2>&1
echo TEST_EXIT=%ERRORLEVEL% >> build-log.txt
cargo check -p dialed >> build-log.txt 2>&1
echo CHECK_EXIT=%ERRORLEVEL% >> build-log.txt
echo Fertig. Ergebnis steht in build-log.txt
pause
