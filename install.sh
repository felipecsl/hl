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
  error "Failed to connect via SSH. Please verify:"
  echo "  1. The hostname/IP is correct"
  echo "  2. SSH key authentication is set up"
  echo "  3. You can manually SSH: ssh ${REMOTE_USER}@${REMOTE_HOST}"
fi
success "SSH connection successful"
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
echo "  1. Make sure the 'hl' binary is installed on the remote server at:"
echo "     ${REMOTE_USER}@${REMOTE_HOST}:~/.local/bin/hl"
echo ""
echo "  2. Test your installation:"
echo "     hl --help"
echo ""
echo "  3. Initialize your first app:"
echo "     hl init --app myapp --image registry.example.com/myapp \\"
echo "       --domain myapp.example.com --port 8080"
echo ""
success "Installation complete!"
