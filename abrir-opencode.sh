#!/bin/bash
# Abre una sesión de OpenCode en la carpeta de ARGUS con el entorno listo
cd "$(dirname "$0")"
source "$HOME/.cargo/env" 2>/dev/null
exec "$HOME/.opencode/bin/opencode" .
