#!/usr/bin/env bash

set -e

echo "Building osu-collect..."
echo ""

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

SCRIPT_DIR="$(dirname "${BASH_SOURCE[0]}")"
PROJECT_DIR="$(cd "${SCRIPT_DIR}" && pwd)"
BUILD_DIR="${PROJECT_DIR}/build"
WINDOWS_TOOLCHAIN_FILE="${BUILD_DIR}/x86_64-w64-mingw32.cmake"
WINDOWS_INCLUDE_DIR="${BUILD_DIR}/windows-include"

echo -e "${BLUE}Setup build directory...${NC}"
rm -rf "${BUILD_DIR}"
mkdir -p "${BUILD_DIR}"

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
echo -e "${BLUE}Version: ${VERSION}${NC}"
echo ""

START_TIME=$(date +%s)

# Build for Linux
echo -e "${GREEN}Building for Linux (x86_64-unknown-linux-gnu)...${NC}"
cargo build --release --target x86_64-unknown-linux-gnu

if [[ -f "target/x86_64-unknown-linux-gnu/release/osu-collect" ]]; then
    cp "target/x86_64-unknown-linux-gnu/release/osu-collect" "${BUILD_DIR}/osu-collect-linux-x64"
    chmod +x "${BUILD_DIR}/osu-collect-linux-x64"
    echo -e "${GREEN}Linux build complete: build/osu-collect-linux-x64${NC}"
else
    echo -e "${YELLOW}Linux binary not found at expected location${NC}"
fi

echo ""

# Build for Windows
echo -e "${GREEN}Building for Windows (x86_64-pc-windows-gnu)...${NC}"

# Check if Windows target is installed
WINDOWS_TARGETS=$(rustup target list)
if ! grep -q "x86_64-pc-windows-gnu (installed)" <<< "${WINDOWS_TARGETS}"; then
   echo -e "${YELLOW}Installing Windows target...${NC}"
   rustup target add x86_64-pc-windows-gnu
fi

# Check if mingw-w64 is available
if ! command -v x86_64-w64-mingw32-gcc &> /dev/null; then
   echo -e "${YELLOW}mingw-w64 not found.${NC}"
   echo ""
fi

mkdir -p "${WINDOWS_INCLUDE_DIR}"
cat > "${WINDOWS_INCLUDE_DIR}/Windows.h" <<'EOF'
#include </usr/x86_64-w64-mingw32/include/windows.h>
EOF
cat > "${WINDOWS_INCLUDE_DIR}/WinSock2.h" <<'EOF'
#include </usr/x86_64-w64-mingw32/include/winsock2.h>
EOF
cat > "${WINDOWS_INCLUDE_DIR}/BaseTsd.h" <<'EOF'
#include </usr/x86_64-w64-mingw32/include/basetsd.h>
EOF
cat > "${WINDOWS_INCLUDE_DIR}/VersionHelpers.h" <<'EOF'
#include </usr/x86_64-w64-mingw32/include/versionhelpers.h>
EOF
: > "${WINDOWS_INCLUDE_DIR}/safeint.h"

cat > "${WINDOWS_TOOLCHAIN_FILE}" <<EOF
set(CMAKE_SYSTEM_NAME Windows)
set(CMAKE_SYSTEM_PROCESSOR x64)
set(CMAKE_C_COMPILER x86_64-w64-mingw32-gcc)
set(CMAKE_CXX_COMPILER x86_64-w64-mingw32-g++)
set(CMAKE_RC_COMPILER x86_64-w64-mingw32-windres)
set(CMAKE_FIND_ROOT_PATH /usr/x86_64-w64-mingw32)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)
set(CMAKE_C_FLAGS_INIT "-I${WINDOWS_INCLUDE_DIR}")
set(CMAKE_CXX_FLAGS_INIT "-I${WINDOWS_INCLUDE_DIR}")
EOF

rm -rf target/x86_64-pc-windows-gnu/release/build/osu-collect-*/out/build
CFLAGS_x86_64_pc_windows_gnu="-I${WINDOWS_INCLUDE_DIR}" CXXFLAGS_x86_64_pc_windows_gnu="-I${WINDOWS_INCLUDE_DIR} -D_USE_MATH_DEFINES -Wno-narrowing -mxsave" CMAKE_TOOLCHAIN_FILE_x86_64_pc_windows_gnu="${WINDOWS_TOOLCHAIN_FILE}" cargo build --release --target x86_64-pc-windows-gnu

if [[ -f "target/x86_64-pc-windows-gnu/release/osu-collect.exe" ]]; then
   cp "target/x86_64-pc-windows-gnu/release/osu-collect.exe" "${BUILD_DIR}/osu-collect-windows-x64.exe"
   echo -e "${GREEN}Windows build complete: build/osu-collect-windows-x64.exe${NC}"
else
   echo -e "${YELLOW}Windows binary not found at expected location${NC}"
fi

echo ""
END_TIME=$(date +%s)
echo -e "${GREEN}Build complete in $(( END_TIME - START_TIME ))s!${NC}"
echo ""
echo "Build artifacts:"
ls -lh "${BUILD_DIR}"
echo ""
echo "Build directory: ${BUILD_DIR}"
