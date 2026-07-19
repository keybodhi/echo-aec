@echo off
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat"
for /f "delims=" %%i in ('where cl.exe') do if not defined REAL_CL set "REAL_CL=%%i"
for /f "delims=" %%i in ('where dumpbin.exe') do if not defined REAL_DUMP set "REAL_DUMP=%%i"
set LIBCLANG_PATH=C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\Llvm\x64\bin
set PATH=C:\tools;C:\Users\Key\AppData\Local\Programs\Python\Python312;C:\Users\Key\AppData\Local\Programs\Python\Python312\Scripts;%PATH%
cd /d C:\echo-aec\audio-core
cargo run --example test_pipewire_config --release 2>&1
