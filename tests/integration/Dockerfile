# Test container for install.sh integration testing
FROM ubuntu:22.04

# Install SSH server and basic tools
RUN apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get install -y \
    openssh-server \
    curl \
    sudo \
    && rm -rf /var/lib/apt/lists/*

# Create SSH directory and configure SSH
RUN mkdir /var/run/sshd && \
    mkdir -p /root/.ssh && \
    chmod 700 /root/.ssh

# Create a test user
RUN useradd -m -s /bin/bash testuser && \
    mkdir -p /home/testuser/.ssh && \
    chmod 700 /home/testuser/.ssh && \
    mkdir -p /home/testuser/.local/bin && \
    chown -R testuser:testuser /home/testuser

# Configure SSH to allow key-based authentication
RUN sed -i 's/#PasswordAuthentication yes/PasswordAuthentication no/' /etc/ssh/sshd_config && \
    sed -i 's/#PubkeyAuthentication yes/PubkeyAuthentication yes/' /etc/ssh/sshd_config && \
    sed -i 's@session\s*required\s*pam_loginuid.so@session optional pam_loginuid.so@g' /etc/pam.d/sshd

# Create a dummy hl binary for testing
RUN echo '#!/bin/bash\necho "hl mock binary - version 0.0.1"' > /home/testuser/.local/bin/hl && \
    chmod +x /home/testuser/.local/bin/hl

# Expose SSH port
EXPOSE 22

CMD ["/usr/sbin/sshd", "-D"]
