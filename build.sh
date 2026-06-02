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
TARGET_DIR="/home/uwuclxdy/repos/rs/target"
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

if [[ -f "${TARGET_DIR}/x86_64-unknown-linux-gnu/release/osu-collect" ]]; then
    cp "${TARGET_DIR}/x86_64-unknown-linux-gnu/release/osu-collect" "${BUILD_DIR}/osu-collect-linux-x64"
    chmod +x "${BUILD_DIR}/osu-collect-linux-x64"
    echo -e "${GREEN}Linux build complete: build/osu-collect-linux-x64${NC}"
else
    echo -e "${YELLOW}Linux binary not found at expected location${NC}"
fi

echo ""

# Build for Windows
echo -e "${GREEN}Building for Windows (x86_64-pc-windows-gnu)...${NC}"

if ! rustup target list --installed | grep -q "x86_64-pc-windows-gnu"; then
    echo -e "${YELLOW}Installing Windows target...${NC}"
    rustup target add x86_64-pc-windows-gnu
fi

if ! command -v x86_64-w64-mingw32-gcc &> /dev/null; then
    echo -e "${YELLOW}mingw-w64 not found. Install with: sudo apt-get install mingw-w64${NC}"
    echo ""
fi

# cmake toolchain so realm-cpp finds zlib in the mingw32 sysroot
# instead of trying to download it from static.realm.io
mkdir -p "${WINDOWS_INCLUDE_DIR}"
printf '#ifndef WIN32_LEAN_AND_MEAN\n#define WIN32_LEAN_AND_MEAN\n#endif\n#include </usr/x86_64-w64-mingw32/include/windows.h>\n' > "${WINDOWS_INCLUDE_DIR}/Windows.h"
printf '#include </usr/x86_64-w64-mingw32/include/winsock2.h>\n' > "${WINDOWS_INCLUDE_DIR}/WinSock2.h"
printf '#include </usr/x86_64-w64-mingw32/include/basetsd.h>\n' > "${WINDOWS_INCLUDE_DIR}/BaseTsd.h"
printf '#include </usr/x86_64-w64-mingw32/include/versionhelpers.h>\n' > "${WINDOWS_INCLUDE_DIR}/VersionHelpers.h"
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
set(ZLIB_ROOT /usr/x86_64-w64-mingw32)
set(CMAKE_C_FLAGS_INIT "-I${WINDOWS_INCLUDE_DIR}")
set(CMAKE_CXX_FLAGS_INIT "-I${WINDOWS_INCLUDE_DIR}")
EOF

rm -rf "${TARGET_DIR}/x86_64-pc-windows-gnu/release/build/osu-collect-*"
rm -rf "${TARGET_DIR}/x86_64-pc-windows-gnu/release/.fingerprint/osu-collect-*"
CFLAGS_x86_64_pc_windows_gnu="-I${WINDOWS_INCLUDE_DIR}" \
    CXXFLAGS_x86_64_pc_windows_gnu="-I${WINDOWS_INCLUDE_DIR} -D_USE_MATH_DEFINES -Wno-narrowing -mxsave" \
    CMAKE_TOOLCHAIN_FILE_x86_64_pc_windows_gnu="${WINDOWS_TOOLCHAIN_FILE}" \
    cargo build --release --target x86_64-pc-windows-gnu

if [[ -f "${TARGET_DIR}/x86_64-pc-windows-gnu/release/osu-collect.exe" ]]; then
   cp "${TARGET_DIR}/x86_64-pc-windows-gnu/release/osu-collect.exe" "${BUILD_DIR}/osu-collect-windows-x64.exe"
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
