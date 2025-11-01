#!/usr/bin/env bash
set -euo pipefail

# Color output helpers
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() {
  echo -e "${BLUE}==>${NC} $1"
}

success() {
  echo -e "${GREEN}✓${NC} $1"
}

warn() {
  echo -e "${YELLOW}!${NC} $1"
}

error() {
  echo -e "${RED}✗${NC} $1"
  exit 1
}

require_command() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    error "Required command '${cmd}' is not available. Please install it and re-run this script."
  fi
}

determine_target_triple() {
  local os="$1"
  local arch="$2"

  case "$os" in
    Linux)
      case "$arch" in
        x86_64 | amd64)
          echo "x86_64-unknown-linux-gnu"
          return
          ;;
        aarch64 | arm64)
          echo "aarch64-unknown-linux-gnu"
          return
          ;;
      esac
      ;;
    Darwin)
      case "$arch" in
        x86_64)
          echo "x86_64-apple-darwin"
          return
          ;;
        arm64 | aarch64)
          echo "aarch64-apple-darwin"
          return
          ;;
      esac
      ;;
  esac

  error "Unsupported remote platform (${os} / ${arch}). Please install hl manually from https://github.com/felipecsl/hl/releases"
}

# Print header
echo ""
info "hl Installation Script"
echo ""
echo "This script will create a wrapper that allows you to run 'hl' commands"
echo "on your remote server via SSH."
echo ""

# Check SSH key authentication notice
warn "IMPORTANT: This script assumes SSH public key authentication is configured."
echo "  Make sure you can SSH to your server without a password prompt."
echo "  If not, set up key-based authentication first:"
echo "    ssh-copy-id user@hostname"
echo ""

for cmd in ssh scp curl tar; do
  require_command "$cmd"
done

read -p "Press Enter to continue or Ctrl+C to abort..."
echo ""

# Prompt for remote user
while true; do
  read -p "Remote SSH username: " REMOTE_USER
  if [ -n "$REMOTE_USER" ]; then
    break
  fi
  error "Username cannot be empty. Please try again."
done

# Prompt for remote host
while true; do
  read -p "Remote hostname or IP: " REMOTE_HOST
  if [ -n "$REMOTE_HOST" ]; then
    break
  fi
  error "Hostname cannot be empty. Please try again."
done

# Test SSH connection
info "Testing SSH connection to ${REMOTE_USER}@${REMOTE_HOST}..."
if ! ssh -o BatchMode=yes -o ConnectTimeout=5 "${REMOTE_USER}@${REMOTE_HOST}" "exit" 2>/dev/null; then
  echo ""
  error "Failed to connect via SSH. Please verify:
  1. The hostname/IP is correct
  2. SSH key authentication is set up
  3. You can manually SSH: ssh ${REMOTE_USER}@${REMOTE_HOST}"
fi
success "SSH connection successful"
echo ""

info "Detecting remote platform..."
REMOTE_OS="$(ssh -o BatchMode=yes "${REMOTE_USER}@${REMOTE_HOST}" "uname -s" | tr -d '\r')"
REMOTE_ARCH="$(ssh -o BatchMode=yes "${REMOTE_USER}@${REMOTE_HOST}" "uname -m" | tr -d '\r')"
if [ -z "$REMOTE_OS" ] || [ -z "$REMOTE_ARCH" ]; then
  error "Failed to detect remote platform details."
fi
info "Remote platform: ${REMOTE_OS} / ${REMOTE_ARCH}"

TARGET_TRIPLE="$(determine_target_triple "$REMOTE_OS" "$REMOTE_ARCH")"
info "Selected release target: ${TARGET_TRIPLE}"

info "Fetching latest hl release metadata..."
LATEST_RELEASE_JSON="$(curl -fsSL -H 'Accept: application/vnd.github+json' https://api.github.com/repos/felipecsl/hl/releases/latest)" || error "Failed to retrieve release information from GitHub."

DOWNLOAD_URL="$(printf '%s\n' "$LATEST_RELEASE_JSON" | sed -n "s/.*\"browser_download_url\": \"\\([^\"]*hl-[^\"]*-${TARGET_TRIPLE}.tar.gz\\)\".*/\\1/p" | head -n 1)"
if [ -z "$DOWNLOAD_URL" ]; then
  error "Unable to resolve download URL for target ${TARGET_TRIPLE}."
fi

ASSET_NAME="$(basename "$DOWNLOAD_URL")"
VERSION="${ASSET_NAME#hl-}"
VERSION="${VERSION%-${TARGET_TRIPLE}.tar.gz}"
if [ -z "$VERSION" ] || [ "$VERSION" = "$ASSET_NAME" ]; then
  error "Failed to parse release version from asset name ${ASSET_NAME}."
fi

info "Downloading hl ${VERSION} (${TARGET_TRIPLE})..."
TMP_DIR="$(mktemp -d)"
cleanup() {
  if [ -n "${TMP_DIR:-}" ] && [ -d "$TMP_DIR" ]; then
    rm -rf "$TMP_DIR"
  fi
}
trap cleanup EXIT
ARCHIVE_PATH="${TMP_DIR}/${ASSET_NAME}"
curl -fsSL "$DOWNLOAD_URL" -o "$ARCHIVE_PATH" || error "Failed to download release artifact."

info "Extracting hl binary..."
tar -C "$TMP_DIR" -xzf "$ARCHIVE_PATH" || error "Failed to extract release archive."
if [ ! -f "${TMP_DIR}/hl" ]; then
  error "Downloaded archive did not contain the hl binary."
fi

info "Ensuring remote ~/.local/bin directory exists..."
ssh "${REMOTE_USER}@${REMOTE_HOST}" "mkdir -p ~/.local/bin" || error "Failed to create ~/.local/bin on remote host."

info "Copying hl binary to remote host..."
scp -q "${TMP_DIR}/hl" "${REMOTE_USER}@${REMOTE_HOST}:~/.local/bin/hl" || error "Failed to copy hl binary to remote host."
ssh "${REMOTE_USER}@${REMOTE_HOST}" "chmod +x ~/.local/bin/hl" || error "Failed to set executable permissions on remote hl binary."
success "hl ${VERSION} installed on ${REMOTE_USER}@${REMOTE_HOST}:~/.local/bin/hl"
echo ""

# Prompt for install location
DEFAULT_INSTALL_DIR="${HOME}/.local/bin"
read -p "Install location [${DEFAULT_INSTALL_DIR}]: " INSTALL_DIR
INSTALL_DIR="${INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"

# Expand tilde if present
INSTALL_DIR="${INSTALL_DIR/#\~/$HOME}"

# Create install directory if it doesn't exist
if [ ! -d "$INSTALL_DIR" ]; then
  info "Creating directory: $INSTALL_DIR"
  mkdir -p "$INSTALL_DIR" || error "Failed to create directory: $INSTALL_DIR"
fi

# Create the wrapper script
WRAPPER_PATH="${INSTALL_DIR}/hl"
info "Creating wrapper script at: $WRAPPER_PATH"

cat > "$WRAPPER_PATH" <<WRAPPER_SCRIPT
#!/usr/bin/env bash
set -euo pipefail
REMOTE_USER="${REMOTE_USER}"
REMOTE_HOST="${REMOTE_HOST}"
ssh "\${REMOTE_USER}@\${REMOTE_HOST}" "~/.local/bin/hl \"\$@\""
WRAPPER_SCRIPT

# Make the wrapper executable
chmod +x "$WRAPPER_PATH" || error "Failed to make wrapper executable"
success "Wrapper script created and made executable"
echo ""

# Check if install directory is in PATH
if [[ ":$PATH:" == *":${INSTALL_DIR}:"* ]]; then
  success "Installation complete! You can now run 'hl' from anywhere."
else
  warn "Installation complete, but ${INSTALL_DIR} is not in your PATH."
  echo ""
  echo "Add the following line to your shell configuration file"
  echo "(~/.bashrc, ~/.zshrc, ~/.bash_profile, or similar):"
  echo ""
  echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
  echo ""
  echo "Then reload your shell or run:"
  echo "    source ~/.bashrc  # or source ~/.zshrc, etc."
  echo ""
fi

# Show next steps
echo ""
info "Next Steps:"
echo "  1. Test your installation:"
echo "     hl --help"
echo ""
echo "  2. Initialize your first app:"
echo "     hl init --app myapp --image registry.example.com/myapp \\"
echo "       --domain myapp.example.com --port 8080"
echo ""
success "Installation complete!"
