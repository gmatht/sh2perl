# Dockerfile for sh2perl test environment
FROM rust:1.75-slim

# Install system dependencies
RUN apt-get update && apt-get install -y \
    perl \
    cpanminus \
    build-essential \
    git \
    bash \
    curl \
    diffutils \
    coreutils \
    && rm -rf /var/lib/apt/lists/*

# Install Perl modules required for testing
# PPI and Perl::Critic are not in core Perl, so they need to be installed
# Getopt::Long, File::Basename, Time::HiRes, and POSIX are core modules
RUN cpanm --notest PPI PPI::Find Perl::Critic

# Set working directory
WORKDIR /workspace

# Copy project files (excluding .dockerignore patterns)
COPY . .

# Build the project (optional - can be done after container starts)
# Commented out to allow connection even if code has compilation errors
# RUN cargo build --bin debashc || cargo build --release --bin debashc

# Set environment variables for consistent testing
ENV LOCALE=C
ENV LC_COLLATE=C
ENV PATH="/workspace/target/debug:/workspace/target/release:$PATH"

# Make test scripts executable
RUN chmod +x ./fail ./test-docker.sh 2>/dev/null || true

# Default command - keep container running for interactive use
# Override with: docker-compose run --rm test bash ./fail
CMD ["bash", "-c", "echo 'Docker test environment ready. Run: bash ./fail' && tail -f /dev/null"]

