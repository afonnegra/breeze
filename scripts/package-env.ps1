#!/usr/bin/env pwsh
#Requires -Version 5.1
# package-env.ps1 - Entorno de build para EMPAQUETADO/RELEASE del instalador (TD-005).
#
# Dot-source antes de un build de release destinado al instalable:
#     . "$PSScriptRoot\package-env.ps1"; flutter build windows --release
#
# --- Por que existe (TD-005) ---
# El build de dev (dev-env.ps1) fija CUDA_COMPUTE_CAP=89 (RTX 4070 Laptop) para
# compilar UNA sola arquitectura y que el ciclo de dev sea rapido. Pero:
#   1. whisper-rs-sys 0.15.0 NO consume CUDA_COMPUTE_CAP (verificado en su
#      build.rs: no hay ninguna referencia). ggml cae al default de CMake
#      `native`, que resuelve a sm_89 en esta maquina.
#   2. Resultado: el DLL solo lleva SASS para sm_89. En cualquier GPU NVIDIA que
#      no sea Ada sm_89, ggml_cuda_init falla y whisper cae a CPU (o no arranca).
# Un instalable para "cualquier GPU NVIDIA con compute capability >= 7.5" necesita
# un fatbin multi-arquitectura. El mecanismo estandar de CMake (>= 3.20) es la
# variable de entorno CUDAARCHS, que inicializa CMAKE_CUDA_ARCHITECTURES. cmake-rs
# (usado por whisper-rs-sys) pasa el entorno del proceso a CMake, asi que basta
# con exportar CUDAARCHS aqui: no requiere parchear el crate.
#
# --- Por que dev sigue en native/sm_89 ---
# dev-env.ps1 queda INTACTO. Compilar 5 arquitecturas tarda mucho mas; se paga
# ese costo solo al empaquetar, no en cada iteracion de dev. Este script
# dot-sourcea dev-env.ps1 (toda la config VS/Ninja/CUDA/PDB) y luego SOBRESCRIBE
# la arquitectura via CUDAARCHS.
#
# --- Rationale de la lista de arquitecturas ---
#   75 -> Turing   (RTX 20xx, GTX 16xx, Quadro RTX)   SASS
#   80 -> Ampere   (A100)                             SASS
#   86 -> Ampere   (RTX 30xx, A40, A10)               SASS
#   89 -> Ada      (RTX 40xx, incl. la 4070 de dev)   SASS
#   90 -> Hopper   (H100)                             SASS + PTX
# El sufijo `-real` genera solo SASS (codigo binario, arranque rapido, sin JIT).
# `90` (sin sufijo) genera SASS **y** PTX: el PTX de la arquitectura mas alta da
# forward-compat via JIT del driver en arquitecturas futuras (Blackwell sm_100+ y
# posteriores), a costa de una compilacion inicial en el primer arranque en esas
# GPUs. Cubre todas las GPUs NVIDIA con compute capability >= 7.5.

. "$PSScriptRoot\dev-env.ps1"

# Multi-arch fatbin. CUDAARCHS inicializa CMAKE_CUDA_ARCHITECTURES (CMake >= 3.20).
$env:CUDAARCHS = '75-real;80-real;86-real;89-real;90'
Write-Host "[package-env] CUDAARCHS = $env:CUDAARCHS (fatbin multi-arch para el instalador)" -ForegroundColor Cyan
