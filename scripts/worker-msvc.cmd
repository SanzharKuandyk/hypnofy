@echo off
call "%HYPNOFY_VSDEVCMD%" -no_logo -arch=x64 -host_arch=x64
if errorlevel 1 exit /b %errorlevel%
cargo %*
exit /b %errorlevel%
