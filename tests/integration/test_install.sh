#!/usr/bin/env bash
set -euo pipefail

# Integration test for install.sh using Docker
# This spins up an SSH server container and tests the installation script

# Color output helpers
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() {
  echo -e "${YELLOW}[TEST]${NC} $1"
}

success() {
  echo -e "${GREEN}[PASS]${NC} $1"
}

error() {
  echo -e "${RED}[FAIL]${NC} $1"
  exit 1
}

# Cleanup function
cleanup() {
  info "Cleaning up..."
  docker stop hl-test-ssh 2>/dev/null || true
  docker rm hl-test-ssh 2>/dev/null || true
  rm -f /tmp/hl-test-key /tmp/hl-test-key.pub
  rm -f /tmp/test-install-modified.sh
  rm -rf /tmp/hl-test-install
}

# Set trap to cleanup on exit
trap cleanup EXIT

# Change to script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

info "Starting integration test for install.sh"
info "Repository root: $REPO_ROOT"

# Step 1: Build the test container
info "Building test container..."
docker build -t hl-test-ssh "$SCRIPT_DIR" || error "Failed to build test container"
success "Test container built"

# Step 2: Generate SSH key for testing
info "Generating SSH key pair..."
ssh-keygen -t rsa -b 2048 -f /tmp/hl-test-key -N "" -C "test@hl" >/dev/null 2>&1 || error "Failed to generate SSH key"
success "SSH key generated"

# Step 3: Start the container with the public key
info "Starting SSH server container..."
docker run -d \
  --name hl-test-ssh \
  -p 2222:22 \
  hl-test-ssh || error "Failed to start container"

# Wait for SSH to be ready
sleep 2

# Copy the public key to the container
info "Setting up SSH authentication..."
docker exec hl-test-ssh bash -c "echo '$(cat /tmp/hl-test-key.pub)' >> /home/testuser/.ssh/authorized_keys"
docker exec hl-test-ssh bash -c "chmod 600 /home/testuser/.ssh/authorized_keys && chown testuser:testuser /home/testuser/.ssh/authorized_keys"
success "SSH authentication configured"

# Step 4: Test SSH connection
info "Testing SSH connection..."
ssh -i /tmp/hl-test-key -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -p 2222 testuser@localhost "echo 'SSH works'" >/dev/null 2>&1 || error "SSH connection failed"
success "SSH connection works"

# Step 5: Test the installation script directly with automated inputs
info "Running install.sh with test inputs..."
mkdir -p /tmp/hl-test-install

# Create a modified version of the install script for testing
cp "$REPO_ROOT/install.sh" /tmp/test-install-modified.sh

# Add SSH override at the beginning (after first line with shebang and set command)
# This override uses the test SSH key and port 2222
# IMPORTANT: Redirect stdin with </dev/null to prevent SSH from consuming pipe input
cat > /tmp/ssh-override.sh <<'SSHOVERRIDE'

# TEST OVERRIDE: Use custom SSH command
ssh() {
  # Redirect stdin to prevent SSH from consuming pipe input
  command ssh -i /tmp/hl-test-key -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -p 2222 "$@" </dev/null
}
SSHOVERRIDE

# Insert after line 2 (after "set -euo pipefail")
sed -i '2r /tmp/ssh-override.sh' /tmp/test-install-modified.sh

# Run the modified install script with automated inputs
{
  echo ""           # Press Enter to continue
  echo "testuser"   # Remote SSH username
  echo "localhost"  # Remote hostname
  echo "/tmp/hl-test-install"  # Install location
} | bash /tmp/test-install-modified.sh 2>&1 | tee /tmp/install-test-output.log

success "Install script executed"

# Step 7: Verify the wrapper was created
info "Verifying wrapper script was created..."
if [ ! -f "/tmp/hl-test-install/hl" ]; then
  error "Wrapper script was not created at /tmp/hl-test-install/hl"
fi
success "Wrapper script exists"

# Step 8: Verify the wrapper is executable
info "Verifying wrapper script is executable..."
if [ ! -x "/tmp/hl-test-install/hl" ]; then
  error "Wrapper script is not executable"
fi
success "Wrapper script is executable"

# Step 9: Verify the wrapper contains the correct SSH details
info "Verifying wrapper script content..."
if ! grep -q "REMOTE_USER=\"testuser\"" /tmp/hl-test-install/hl; then
  error "Wrapper does not contain correct username"
fi
if ! grep -q "REMOTE_HOST=\"localhost\"" /tmp/hl-test-install/hl; then
  error "Wrapper does not contain correct hostname"
fi
success "Wrapper script content is correct"

# Step 10: Test the wrapper (note: this would fail without proper SSH port override, so we skip actual execution)
info "Verifying wrapper script structure..."
if ! head -1 /tmp/hl-test-install/hl | grep -q "#!/usr/bin/env bash"; then
  error "Wrapper does not have correct shebang"
fi
success "Wrapper script has correct structure"

info ""
success "All integration tests passed! ✓"
info ""
info "Test artifacts:"
info "  - Container: hl-test-ssh (will be cleaned up)"
info "  - Wrapper: /tmp/hl-test-install/hl"
info "  - Test log: /tmp/install-test-output.log"
