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

ensure_git_repo() {
  if ! git rev-parse --git-dir >/dev/null 2>&1; then
    error "This script must be run from within the hl git repository."
  fi
}

# Print header
echo ""
info "hl Installation Script (HEAD build)"
echo ""
echo "This script installs hl from your current local git commit onto a remote"
echo "server by building locally, uploading the binary, and creating a local"
echo "SSH wrapper."
echo ""

# Check SSH key authentication notice
warn "IMPORTANT: This script assumes SSH public key authentication is configured."
echo "  Make sure you can SSH to your server without a password prompt."
echo "  If not, set up key-based authentication first:"
echo "    ssh-copy-id user@hostname"
echo ""

for cmd in git ssh scp tar cargo rustc; do
  require_command "$cmd"
done
ensure_git_repo

COMMIT_SHA="$(git rev-parse --short=12 HEAD 2>/dev/null)" || error "Failed to read current git commit."
if [ -z "${COMMIT_SHA}" ]; then
  error "Failed to determine current git commit."
fi

info "Current local commit: ${COMMIT_SHA}"
warn "Only committed files in HEAD will be included (uncommitted changes are ignored)."
warn "This script assumes local and remote hosts use the same OS/architecture."
echo ""

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

TMP_DIR="$(mktemp -d)"
cleanup() {
  if [ -n "${TMP_DIR:-}" ] && [ -d "$TMP_DIR" ]; then
    rm -rf "$TMP_DIR"
  fi
}
trap cleanup EXIT

SOURCE_ARCHIVE="${TMP_DIR}/hl-${COMMIT_SHA}.tar.gz"
BUILD_DIR="${TMP_DIR}/src"
LOCAL_BINARY_PATH="${BUILD_DIR}/target/release/hl"

info "Creating source archive from HEAD..."
git archive --format=tar.gz -o "$SOURCE_ARCHIVE" HEAD || error "Failed to archive current git commit."

info "Extracting source archive..."
mkdir -p "$BUILD_DIR"
tar -C "$BUILD_DIR" -xzf "$SOURCE_ARCHIVE" || error "Failed to extract source archive."

info "Building hl locally from commit ${COMMIT_SHA}..."
(
  cd "$BUILD_DIR"
  cargo build --release --locked
) || error "Local build failed."

if [ ! -f "$LOCAL_BINARY_PATH" ]; then
  error "Local build completed but binary was not found at ${LOCAL_BINARY_PATH}."
fi

info "Ensuring remote ~/.local/bin directory exists..."
ssh "${REMOTE_USER}@${REMOTE_HOST}" "mkdir -p ~/.local/bin" || error "Failed to create ~/.local/bin on remote host."

info "Copying locally built binary to remote host..."
scp -q "$LOCAL_BINARY_PATH" "${REMOTE_USER}@${REMOTE_HOST}:~/.local/bin/hl" || error "Failed to copy hl binary to remote host."

info "Setting executable permissions on remote binary..."
ssh "${REMOTE_USER}@${REMOTE_HOST}" "chmod +x ~/.local/bin/hl" || error "Failed to set executable permissions on remote hl binary."

info "Verifying remote hl binary..."
REMOTE_SCRIPT_OUTPUT="$(ssh "${REMOTE_USER}@${REMOTE_HOST}" "~/.local/bin/hl --version")" || error "Installed remote binary did not execute successfully."

success "hl ${COMMIT_SHA} installed on ${REMOTE_USER}@${REMOTE_HOST}:~/.local/bin/hl"
if [ -n "$REMOTE_SCRIPT_OUTPUT" ]; then
  info "Remote version: ${REMOTE_SCRIPT_OUTPUT}"
fi
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

infer_hl_app() {
  local -a apps
  mapfile -t apps < <(
    git remote -v 2>/dev/null \
      | awk '{print $2}' \
      | sed -nE 's#.*[:/]hl/git/([^/]+)\.git\$#\1#p' \
      | sort -u
  )

  case "\${#apps[@]}" in
    0) return 0 ;;
    1) printf '%s\n' "\${apps[0]}" ;;
    *)
      echo "Error: multiple hl remotes found (\${apps[*]}). Set HL_APP explicitly." >&2
      exit 1
      ;;
  esac
}

APP_NAME="\${HL_APP:-}"
if [ -z "\$APP_NAME" ]; then
  APP_NAME="\$(infer_hl_app || true)"
fi

if [ -n "\$APP_NAME" ]; then
  ssh "\${REMOTE_USER}@\${REMOTE_HOST}" HL_APP="\$APP_NAME" ~/.local/bin/hl "\$@"
else
  ssh "\${REMOTE_USER}@\${REMOTE_HOST}" ~/.local/bin/hl "\$@"
fi
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

echo ""
info "Next Steps:"
echo "  1. Test your installation:"
echo "     hl --help"
echo ""
echo "  2. Confirm installed commit on remote:"
echo "     ssh ${REMOTE_USER}@${REMOTE_HOST} ~/.local/bin/hl --version"
echo ""
success "HEAD installation complete!"
