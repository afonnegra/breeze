#!/usr/bin/env pwsh
#Requires -Version 5.1
# dev-env.ps1 — Configura el entorno de build para inputVoice (ritual TD-001).
# Dot-source desde otros scripts:  . "$PSScriptRoot\dev-env.ps1"
# Ver docs/BUILDING.md para la explicación de cada paso.

# 1. Activar entorno VS 2026 (cl.exe, SDK, Ninja en PATH)
$vcvars = "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path $vcvars)) {
    throw "vcvars64.bat no encontrado en '$vcvars'. ¿Se movió la instalación de Visual Studio?"
}
cmd /c "`"$vcvars`" && set" | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)') {
        [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process')
    }
}

# 2. VSINSTALLDIR es incompatible con generator Ninja (cmake-rs lo mapea a
#    -DCMAKE_GENERATOR_INSTANCE, que Ninja rechaza). Removerla SIEMPRE.
Remove-Item Env:\VSINSTALLDIR -ErrorAction SilentlyContinue

# 3. Forzar Ninja: los generators MSBuild fallan con VS 2026 (toolset v180
#    desconocido para CMake) y con CUDA 13.2.
$env:CMAKE_GENERATOR = "Ninja"
Remove-Item Env:\CMAKE_GENERATOR_PLATFORM -ErrorAction SilentlyContinue

# 4. Asegurar ninja.exe en PATH (vcvars de VS 2026 normalmente ya lo añade)
if (-not (Get-Command ninja -ErrorAction SilentlyContinue)) {
    $ninja = Get-ChildItem "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja\" -Filter ninja.exe -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($ninja) { $env:PATH = "$($ninja.Directory.FullName);$env:PATH" }
    else { throw "ninja.exe no encontrado. Instalar Ninja o reparar VS 2026." }
}

# 5. Env vars CUDA (vcvars puede haberlas pisado). CUDA_COMPUTE_CAP=89 = RTX 4070 Laptop,
#    evita compilar para todas las capabilities (~6x más rápido).
#
#    Selección de toolkit (TD-003): el driver instalado (561.00) soporta CUDA
#    hasta 12.6. Compilar contra 13.2 hace que ggml_cuda_init falle en runtime
#    y whisper caiga a CPU silenciosamente. Además, compilar contra 12.x
#    maximiza la compatibilidad del instalable con drivers de usuario final.
#    Preferimos 12.6 si está instalado; si no, caemos a 13.2 (build compila
#    pero correrá en CPU hasta instalar 12.6 o actualizar el driver a r580+).
$cudaCandidates = @(
    "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6",
    "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.5",
    "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.4",
    "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.2"
)
$cudaPath = $cudaCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cudaPath) { throw "No se encontró ningún CUDA Toolkit en las rutas conocidas." }
$env:CUDA_PATH = $cudaPath
$env:CUDA_COMPUTE_CAP = "89"
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
Write-Host "[dev-env] CUDA toolkit: $cudaPath" -ForegroundColor DarkGray

# 5b. CUDA 12.x no reconoce el MSVC de VS 2026 (toolset 14.50, _MSC_VER 1950):
#     host_config.h aborta con C1189 "unsupported Microsoft Visual Studio
#     version" (detectado 2026-07-02, primer rebuild contra 12.6 — TD-003).
#     No hay toolset 14.4x instalado como host alternativo, así que se
#     desactiva el check de versión de nvcc. Riesgo aceptado: combinación no
#     certificada por NVIDIA, pero es el workaround estándar del ecosistema
#     (llama.cpp/whisper.cpp) y el resultado se valida con los tests de
#     integración GPU. NVCC_PREPEND_FLAGS llega a TODA invocación de nvcc
#     (incluida la detección de compilador de CMake); CUDAFLAGS siembra
#     CMAKE_CUDA_FLAGS por si algún paso ignora la primera.
if ((Split-Path $cudaPath -Leaf) -like "v12.*") {
    $env:NVCC_PREPEND_FLAGS = "-allow-unsupported-compiler"
    $env:CUDAFLAGS = "-allow-unsupported-compiler"
    Write-Host "[dev-env] nvcc: -allow-unsupported-compiler (MSVC 14.50 no certificado para CUDA 12.x)" -ForegroundColor DarkGray
}

# 6. /FS: serializa escrituras de PDB vía mspdbsrv. Defensa secundaria contra
#    C1041 en builds C/C++ de whisper.cpp. Ver TD-002.
$env:CFLAGS = "/FS"
$env:CXXFLAGS = "/FS"

# 6b. CAUSA RAÍZ de C1041 bajo cargokit (TD-002): la ruta default
#     <proyecto>\build\windows\x64\plugins\...\cargokit_build\... produce PDBs
#     de 277 chars > MAX_PATH (260). mspdbsrv no soporta rutas largas.
#     cargokit.cmake (parcheado) respeta este override hacia una ruta corta.
$env:CARGOKIT_TEMP_DIR_OVERRIDE = "C:\dev\ivb"

# 7. cargo y flutter_rust_bridge_codegen viven en ~/.cargo/bin (no siempre en PATH)
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if ($env:PATH -notlike "*$cargoBin*") { $env:PATH = "$cargoBin;$env:PATH" }

Write-Host "[dev-env] VS 2026 + Ninja + CUDA $(Split-Path $cudaPath -Leaf) (sm_89) listos." -ForegroundColor DarkGray
