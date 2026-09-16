#!/usr/bin/env bash
set -euo pipefail

echo "=========================================================="
echo "   Flutter Native MCP - Test & Quality Harness Runner     "
echo "=========================================================="

echo -e "\n[1/5] Verificando formato de código (cargo fmt)..."
cargo fmt --check || {
    echo "⚠️ Formato desalineado. Ejecuta 'cargo fmt' para corregir."
    exit 1
}

echo -e "\n[2/5] Ejecutando análisis estático (cargo clippy)..."
cargo clippy --all-targets -- -D warnings

echo -e "\n[3/5] Ejecutando suite de pruebas unitarias e integración..."
cargo test --all-targets

echo -e "\n[4/5] Ejecutando auditoría de seguridad de dependencias (cargo audit)..."
if command -v cargo-audit &> /dev/null; then
    cargo audit
else
    echo "⚠️ cargo-audit no está instalado en PATH. Se recomienda: cargo install cargo-audit"
fi

echo -e "\n[5/5] Verificando soporte de Mutation Testing (cargo-mutants)..."
if command -v cargo-mutants &> /dev/null; then
    echo "Ejecutando mutaciones de prueba sobre el dominio..."
    cargo mutants --package flutter-native-mcp --file src/domain/
else
    echo "ℹ️ cargo-mutants no está instalado globalmente. Para ejecutar mutation testing:"
    echo "   cargo install cargo-mutants && cargo mutants"
fi

echo -e "\n=========================================================="
echo "  ✅ Todos los gates de calidad y seguridad pasaron con éxito!"
echo "=========================================================="
