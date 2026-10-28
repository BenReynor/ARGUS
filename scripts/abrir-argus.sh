#!/usr/bin/env bash
# Abre ARGUS compilado desde la última versión de `main`, recompilándolo antes
# si `main` cambió. Lo usan el acceso directo y el hook post-merge de git.
#
#   abrir-argus.sh             actualiza si hace falta y abre la app
#   abrir-argus.sh --compilar  solo actualiza (en segundo plano, sin abrir)
#
# Compila en una copia aparte del repo (git worktree) que sigue a `main`, así
# nunca empaqueta trabajo a medias de otra rama (p. ej. un loop de Dédalo).
set -u

REPO="$(cd "$(dirname "$0")/.." && pwd)"
DATOS="${XDG_DATA_HOME:-$HOME/.local/share}/argus-app"
FUENTE="$DATOS/fuente"
BIN="$DATOS/argus"
VERSION="$DATOS/version"
LOG="$DATOS/compilacion.log"
mkdir -p "$DATOS"

# Un hook de git exporta GIT_DIR & co.: apuntarían al repo equivocado.
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE
# El acceso directo no hereda el PATH de la terminal (cargo, node de nvm).
export PATH="$HOME/.cargo/bin:$PATH"
if [ -s "$HOME/.nvm/nvm.sh" ]; then
    # shellcheck disable=SC1091
    . "$HOME/.nvm/nvm.sh" >/dev/null 2>&1
fi

aviso() {
    command -v notify-send >/dev/null && notify-send -i "$REPO/src-tauri/icons/icon.png" "ARGUS" "$1"
}

# Huella del código de la app en `main`: cambios en el plan o la documentación
# (Dédalo commitea plan.md en cada tarea) no obligan a recompilar.
FUENTES_APP=(src src-tauri package.json package-lock.json index.html vite.config.ts tsconfig.json)
huella() {
    local p
    for p in "${FUENTES_APP[@]}"; do
        git -C "$REPO" rev-parse "main:$p" 2>/dev/null || echo "-"
    done | sha1sum | cut -d' ' -f1
}

al_dia() {
    [ -x "$BIN" ] && [ -f "$VERSION" ] && [ "$(cat "$VERSION")" = "$(huella)" ]
}

actualizar() {
    al_dia && return 0    # camino rápido: no hace falta ni el cerrojo
    # El cerrojo vive solo en este subshell: si la app lo heredara al abrirse,
    # bloquearía todas las recompilaciones mientras estuviera abierta.
    (
    flock 9    # si el hook ya está compilando, espera a que termine
    al_dia && exit 0
    local sha marca
    sha="$(git -C "$REPO" rev-parse main)"
    marca="$(huella)"
    {
        echo "== $(date '+%F %T') compilando $sha"
        if [ ! -d "$FUENTE/.git" ] && [ ! -f "$FUENTE/.git" ]; then
            git -C "$REPO" worktree prune
            git -C "$REPO" worktree add --detach "$FUENTE" "$sha" || return 1
        else
            git -C "$FUENTE" checkout -q -f --detach "$sha" || return 1
        fi
        cd "$FUENTE" || return 1
        local lock_npm
        lock_npm="$(sha1sum package-lock.json | cut -d' ' -f1)"
        if [ ! -d node_modules ] || [ "$(cat "$DATOS/npm" 2>/dev/null)" != "$lock_npm" ]; then
            npm ci || return 1
            echo "$lock_npm" > "$DATOS/npm"
        fi
        npm run tauri build -- --no-bundle || return 1
        cp src-tauri/target/release/argus "$BIN.nuevo" && mv -f "$BIN.nuevo" "$BIN" || return 1
        echo "$marca" > "$VERSION"
        echo "== $(date '+%F %T') listo"
    } >>"$LOG" 2>&1
    ) 9>"$DATOS/lock"
}

if [ "${1:-}" = "--compilar" ]; then
    actualizar
    exit $?
fi

hay_pantalla() {
    command -v zenity >/dev/null && [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]
}

# Aviso visible que se cierra solo (la notificación del sistema puede pasar
# desapercibida y el usuario vuelve a hacer clic).
aviso_visible() {
    if hay_pantalla; then
        zenity --info --title=ARGUS --width=360 --timeout=6 --text="$1" 2>/dev/null
    else
        aviso "$1"
    fi
}

# Ejecuta "$@" mostrando una ventana de espera con el paso actual de la
# compilación (sale del log). Devuelve el código de "$@".
con_ventana_espera() {
    if ! hay_pantalla; then
        aviso "Actualizando a la última versión… (unos minutos)"
        "$@"
        return $?
    fi
    "$@" &
    local pid=$! paso inicio=$SECONDS
    {
        while kill -0 "$pid" 2>/dev/null; do
            paso="$(grep -aE '^== |Compiling |Finished|added [0-9]+ packages|vite v|built in' "$LOG" 2>/dev/null \
                | tail -n 1 | sed 's/^ *//; s/^== [0-9: -]*//; s/^Compiling/Compilando/; s/^Finished/Terminado/' \
                | tr -d '<>&\\' | cut -c1-70)"
            # Una sola línea por mensaje: zenity convierte los \n escritos en saltos.
            printf '# Actualizando ARGUS a la última versión…\\nNo cierres esta ventana: ARGUS se abrirá solo al terminar.\\n\\n%s  (%ss)\n' \
                "${paso:-Preparando…}" "$((SECONDS - inicio))"
            sleep 1
        done
        echo 100
    } | zenity --progress --pulsate --auto-close --no-cancel --title="ARGUS" \
          --width=480 --text="Actualizando ARGUS…" 2>/dev/null
    wait "$pid"
}

# Un solo arranque a la vez: los clics extra mientras se actualiza o mientras
# la app está abierta no abren ventanas duplicadas (la app hereda el fd 8).
exec 8>"$DATOS/abrir.lock"
if ! flock -n 8; then
    aviso_visible "ARGUS ya está abierto o se está actualizando.\n\nNo hace falta volver a hacer clic."
    exit 0
fi

if ! al_dia; then
    if ! con_ventana_espera actualizar; then
        msg="No se pudo compilar la versión nueva. Detalle en $LOG"
        if [ -x "$BIN" ]; then
            hay_pantalla && zenity --warning --title=ARGUS \
                --text="$msg\n\nSe abre la versión anterior." 2>/dev/null
        else
            hay_pantalla && zenity --error --title=ARGUS --text="$msg" 2>/dev/null
            exit 1
        fi
    fi
fi
exec 9>&- 2>/dev/null   # por si algún día se abre aquí: la app no hereda el cerrojo de compilación
exec "$BIN" "$@"
