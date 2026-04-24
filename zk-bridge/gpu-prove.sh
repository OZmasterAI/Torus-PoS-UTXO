#!/bin/bash
# GPU Groth16 Proof Generation Script
# Run this on a Clore.ai (or any) CUDA GPU instance
#
# Usage:
#   1. Rent an RTX 4090 instance on clore.ai
#   2. Upload this entire zk-bridge/ directory to the instance
#   3. SSH in and run: bash gpu-prove.sh
#   4. Copy back: groth16_onchain_proof.hex
#
# Expected time: ~10-30 min total (install + prove)
# Expected cost: ~$0.30-0.70

set -euo pipefail

echo "=== Torus ZK Bridge — GPU Groth16 Setup ==="
echo ""

# Step 1: Check for NVIDIA GPU
echo "[1/5] Checking GPU..."
if ! nvidia-smi &>/dev/null; then
    echo "ERROR: No NVIDIA GPU detected. nvidia-smi failed."
    echo "Make sure you rented a GPU instance with CUDA support."
    exit 1
fi
nvidia-smi --query-gpu=name,memory.total --format=csv,noheader
echo ""

# Step 2: Install Rust (if not present)
echo "[2/5] Checking Rust..."
if ! command -v cargo &>/dev/null; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi
rustc --version
echo ""

# Step 3: Install SP1 toolchain
echo "[3/5] Installing SP1 toolchain..."
if ! command -v cargo-prove &>/dev/null; then
    curl -L https://sp1up.dev | bash
    source "$HOME/.bashrc" 2>/dev/null || source "$HOME/.profile" 2>/dev/null || true
    export PATH="$HOME/.sp1/bin:$PATH"
    sp1up
fi
cargo-prove prove --version 2>/dev/null || cargo prove --version 2>/dev/null || echo "SP1 installed"
echo ""

# Step 4: Build and run Groth16 prover
echo "[4/5] Building and generating Groth16 proof..."
echo "This will take ~10-20 minutes. Do not interrupt."
echo ""

cd "$(dirname "$0")"

cargo run --release --bin groth16_prove --features cuda 2>&1 | tee groth16_output.log

# Step 5: Verify output files
echo ""
echo "[5/5] Checking output files..."
for f in groth16_onchain_proof.hex groth16_proof.json groth16_proof.bin groth16_public_values.bin; do
    if [ -f "$f" ]; then
        echo "  OK: $f ($(wc -c < "$f") bytes)"
    else
        echo "  MISSING: $f"
    fi
done

echo ""
echo "=== DONE ==="
echo ""
echo "Copy these files back to your local machine:"
echo "  scp <gpu-host>:$(pwd)/groth16_onchain_proof.hex ."
echo "  scp <gpu-host>:$(pwd)/groth16_proof.json ."
echo ""
echo "groth16_onchain_proof.hex is the proof parameter for WrappedTRS.mint()"
