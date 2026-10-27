#!/bin/bash
# Lanza ARGUS en modo desarrollo desde la carpeta del proyecto.
cd "$(dirname "$0")"
exec npm run tauri dev
