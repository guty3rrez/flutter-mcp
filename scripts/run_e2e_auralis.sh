#!/usr/bin/env bash
set -euo pipefail

AURALIS_DIR="/home/guty_3rrez/Proyectos/auralis"
AURALIS_BUNDLE="$AURALIS_DIR/build/linux/x64/debug/bundle/auralis"
LOG_FILE="/tmp/auralis_e2e_run.log"
AURALIS_PID=""

echo "=========================================================="
echo "    Flutter MCP - Runner de Pruebas E2E en Vivo (Auralis) "
echo "=========================================================="

USE_FLUTTER_RUN=false
for arg in "$@"; do
    case "$arg" in
        --flutter-run|--from-scratch|--compile)
            USE_FLUTTER_RUN=true
            shift
            ;;
    esac
done

cleanup() {
    echo -e "\n🧹 Limpiando procesos de Auralis..."
    if [[ -n "$AURALIS_PID" ]] && kill -0 "$AURALIS_PID" 2>/dev/null; then
        kill "$AURALIS_PID" 2>/dev/null || true
        wait "$AURALIS_PID" 2>/dev/null || true
    fi
    pkill -f "build/linux/x64/debug/bundle/auralis" 2>/dev/null || true
    echo "✅ Proceso Auralis finalizado."
}
trap cleanup EXIT INT TERM

# Asegurar que no haya instancias colgadas previas
pkill -f "build/linux/x64/debug/bundle/auralis" 2>/dev/null || true

> "$LOG_FILE"

if [[ "$USE_FLUTTER_RUN" == true ]]; then
    echo -e "\n[1/3] Compilando y ejecutando Auralis desde cero (flutter run -d linux -t lib/main_driver.dart)..."
    (cd "$AURALIS_DIR" && flutter run -d linux -t lib/main_driver.dart > "$LOG_FILE" 2>&1) &
    AURALIS_PID=$!
else
    if [[ ! -f "$AURALIS_BUNDLE" ]]; then
        echo "⚠️ El bundle precompilado no existe en $AURALIS_BUNDLE."
        echo "   Compilando con 'flutter build linux --debug -t lib/main_driver.dart'..."
        (cd "$AURALIS_DIR" && flutter build linux --debug -t lib/main_driver.dart)
    fi
    echo -e "\n[1/3] Lanzando bundle debug de Auralis..."
    "$AURALIS_BUNDLE" > "$LOG_FILE" 2>&1 &
    AURALIS_PID=$!
fi

echo -e "\n[2/3] Esperando inicio del Dart VM Service..."
MAX_WAIT_SECONDS=25
if [[ "$USE_FLUTTER_RUN" == true ]]; then
    MAX_WAIT_SECONDS=120
fi

VM_HTTP_URL=""
for ((i=1; i<=MAX_WAIT_SECONDS*2; i++)); do
    if grep -E "(The Dart VM service is listening on|A Dart VM Service on Linux is available at:)" "$LOG_FILE" > /dev/null 2>&1; then
        VM_HTTP_URL=$(grep -Eo "http://127.0.0.1:[0-9]+/[^ /]+" "$LOG_FILE" | head -n 1 || true)
        if [[ -n "$VM_HTTP_URL" ]]; then
            break
        fi
    fi
    sleep 0.5
done

if [[ -z "$VM_HTTP_URL" ]]; then
    echo "❌ Error: No se pudo obtener la URL de Dart VM Service en ${MAX_WAIT_SECONDS} segundos."
    echo "--- Últimos logs de Auralis ---"
    cat "$LOG_FILE"
    exit 1
fi

VM_WS_URI="${VM_HTTP_URL/http:\/\//ws:\/\/}/ws"
echo "✅ Dart VM Service activo en: $VM_WS_URI"

echo -e "\n[3/3] Ejecutando suite E2E en vivo (cargo test --test e2e_live_auralis)..."
export AURALIS_VM_URI="$VM_WS_URI"
cargo test --test e2e_live_auralis -- --ignored --nocapture

echo -e "\n=========================================================="
echo "  ✅ Prueba E2E en vivo contra Auralis completada con éxito!"
echo "=========================================================="
