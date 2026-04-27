#!/usr/bin/env bash
# Medio install script
# Usage: curl -fsSL https://raw.githubusercontent.com/3kaiu/Medio/main/install.sh | bash
# Optional: -s latest for main branch, -s 1.0.0 for specific version
set -euo pipefail

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log_info()    { echo -e "${BLUE}|${NC} $1"; }
log_success() { echo -e "${GREEN}✓${NC} $1"; }
log_warning() { echo -e "${YELLOW}!${NC} $1"; }
log_error()   { echo -e "${RED}✗${NC} $1"; }

INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="$HOME/.config/medio"
REPO="3kaiu/Medio"
BINARY="medio"
ALIAS="me"
VERSION=""

# Parse args
SOURCE="latest"
while [[ $# -gt 0 ]]; do
    case "$1" in
        -s|--source) SOURCE="$2"; shift 2 ;;
        -d|--dir)    INSTALL_DIR="$2"; shift 2 ;;
        -h|--help)
            echo "Usage: curl -fsSL .../install.sh | bash [-s latest|1.0.0] [-d /usr/local/bin]"
            exit 0 ;;
        *) shift ;;
    esac
done

needs_sudo() { [[ ! -w "$INSTALL_DIR" ]]; }
maybe_sudo() { if needs_sudo; then sudo "$@"; else "$@"; fi; }

get_latest_version() {
    local tag
    tag=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
        | grep '"tag_name"' | head -1 | sed -E 's/.*"v?([^"]+)".*/\1/')
    echo "${tag:-0.1.0}"
}

get_arch_suffix() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        arm64)  echo "aarch64" ;;
        x86_64) echo "x86_64" ;;
        *)      echo "$arch" ;;
    esac
}

get_os() {
    case "$(uname -s)" in
        Darwin) echo "darwin" ;;
        Linux)  echo "linux" ;;
        *)      echo "$(uname -s | tr '[:upper:]' '[:lower:]')" ;;
    esac
}

download_release() {
    local version="$1"
    local os="$2"
    local arch="$3"
    local target="$4"

    local url="https://github.com/${REPO}/releases/download/v${version}/${BINARY}-${os}-${arch}"

    log_info "Downloading ${BINARY} v${version} for ${os}-${arch}..."

    if curl -fsSL --connect-timeout 10 --max-time 120 -o "$target" "$url"; then
        chmod +x "$target"
        xattr -c "$target" 2>/dev/null || true
        log_success "Downloaded ${BINARY} v${version}"
        return 0
    fi

    log_error "Failed to download ${BINARY} v${version}"
    return 1
}

build_from_source() {
    local target="$1"

    if ! command -v cargo > /dev/null 2>&1; then
        log_error "cargo not found. Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        return 1
    fi

    log_info "Building ${BINARY} from source..."

    local tmpdir
    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT

    git clone --depth 1 "https://github.com/${REPO}.git" "$tmpdir/medio" 2>/dev/null

    if (cd "$tmpdir/medio" && cargo build --release --quiet 2>/dev/null); then
        cp "$tmpdir/medio/target/release/${BINARY}" "$target"
        chmod +x "$target"
        log_success "Built ${BINARY} from source"
        return 0
    fi

    log_error "Failed to build from source"
    return 1
}

install_binary() {
    local target="${INSTALL_DIR}/${BINARY}"

    if [[ -z "$VERSION" ]]; then
        if [[ "$SOURCE" == "latest" ]]; then
            VERSION=$(get_latest_version)
        else
            VERSION="$SOURCE"
        fi
    fi

    local os
    os=$(get_os)
    local arch
    arch=$(get_arch_suffix)

    # Try prebuilt binary first
    local tmpfile
    tmpfile=$(mktemp)

    if download_release "$VERSION" "$os" "$arch" "$tmpfile"; then
        if needs_sudo; then
            log_info "Admin access required for ${INSTALL_DIR}"
            sudo -v
        fi
        maybe_sudo cp "$tmpfile" "${target}.new"
        maybe_sudo chmod +x "${target}.new"
        maybe_sudo mv -f "${target}.new" "$target"
        rm -f "$tmpfile"
    elif build_from_source "$tmpfile"; then
        if needs_sudo; then
            log_info "Admin access required for ${INSTALL_DIR}"
            sudo -v
        fi
        maybe_sudo cp "$tmpfile" "${target}.new"
        maybe_sudo chmod +x "${target}.new"
        maybe_sudo mv -f "${target}.new" "$target"
        rm -f "$tmpfile"
    else
        rm -f "$tmpfile"
        log_error "Installation failed. Try: cargo install medio --git https://github.com/${REPO}"
        exit 1
    fi

    log_success "Installed ${BINARY} to ${target}"
}

install_alias() {
    local target="${INSTALL_DIR}/${ALIAS}"

    if needs_sudo; then
        sudo -v 2>/dev/null || return
    fi

    maybe_sudo bash -c "cat > '${target}.new' << 'EOF'
#!/usr/bin/env bash
exec ${BINARY} \"\\\$@\"
EOF"
    maybe_sudo chmod +x "${target}.new"
    maybe_sudo mv -f "${target}.new" "$target"

    log_success "Installed ${ALIAS} alias (run 'me' instead of 'medio')"
}

setup_config_dir() {
    mkdir -p "$CONFIG_DIR"
}

verify_install() {
    if command -v "$BINARY" > /dev/null 2>&1; then
        local v
        v=$("$BINARY" --version 2>/dev/null || echo "unknown")
        log_success "medio ${v} installed successfully"
    else
        log_warning "medio not found in PATH. Add ${INSTALL_DIR} to your PATH:"
        echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    fi
}

# Main
echo ""
echo -e "${BLUE}  __  __       _       _${NC}"
echo -e "${BLUE} |  \\/  | __ _| |_ ___| |__${NC}"
echo -e "${BLUE} | |\\/| |/ _\` | __/ __| '_ \\${NC}"
echo -e "${BLUE} | |  | | (_| | |_\\__ \\ | | |${NC}"
echo -e "${BLUE} |_|  |_|\\__,_|\\__|___/_| |_|${NC}"
echo ""

install_binary
install_alias
setup_config_dir
verify_install

echo ""
log_success "Done! Run 'me' or 'medio --help' to get started."
