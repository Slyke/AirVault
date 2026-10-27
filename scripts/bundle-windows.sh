#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$PWD/.tools/bin:$HOME/.cargo/bin:$PATH"
export NVM_DIR="$HOME/.nvm"
source "$NVM_DIR/nvm.sh"
nvm use 24 >/dev/null
cp .tools/bin/makensis.exe .tools/bin/makensis
chmod +x .tools/bin/makensis
npm run tauri -- bundle --target x86_64-pc-windows-gnu --bundles nsis
