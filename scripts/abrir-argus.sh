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

al_dia() {
    [ -x "$BIN" ] && [ -f "$VERSION" ] && [ "$(cat "$VERSION")" = "$(git -C "$REPO" rev-parse main)" ]
}

actualizar() {
    exec 9>"$DATOS/lock"
    flock 9    # si el hook ya está compilando, espera a que termine
    al_dia && return 0
    local sha
    sha="$(git -C "$REPO" rev-parse main)"
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
        echo "$sha" > "$VERSION"
        echo "== $(date '+%F %T') listo"
    } >>"$LOG" 2>&1
}

if [ "${1:-}" = "--compilar" ]; then
    actualizar
    exit $?
fi

if ! al_dia; then
    aviso "Actualizando a la última versión… (unos minutos)"
    if ! actualizar; then
        msg="No se pudo compilar la versión nueva. Detalle en $LOG"
        if [ -x "$BIN" ]; then
            command -v zenity >/dev/null && zenity --warning --title=ARGUS \
                --text="$msg\n\nSe abre la versión anterior." 2>/dev/null
        else
            command -v zenity >/dev/null && zenity --error --title=ARGUS --text="$msg" 2>/dev/null
            exit 1
        fi
    fi
fi
exec "$BIN" "$@"
