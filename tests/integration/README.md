# Integration Tests for hl

This directory contains integration tests for the `hl` project.

## Installation Script Test

The `test_install.sh` script tests the `install.sh` installation script using Docker.

### Prerequisites

- Docker installed and running
- Bash shell

### How It Works

1. Builds a Docker container with an SSH server
2. Generates an SSH key pair for testing
3. Configures the container with the test SSH key
4. Runs the `install.sh` script with automated inputs
5. Verifies the wrapper script was created correctly
6. Cleans up all test artifacts

### Running the Test

```bash
cd tests/integration
./test_install.sh
```

### What It Tests

- SSH connection validation
- Interactive prompts (automated with test inputs)
- Wrapper script creation
- Executable permissions
- Correct SSH configuration in wrapper
- PATH handling

### Notes

- The test uses port 2222 for SSH to avoid conflicts
- Test artifacts are automatically cleaned up on exit
- The test creates a mock `hl` binary in the container for testing purposes
