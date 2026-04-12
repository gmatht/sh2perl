# Docker Test Environment

This document describes how to use Docker for testing the sh2perl project in a consistent, isolated environment.

## Prerequisites

- Docker installed and running
- Docker Compose (usually included with Docker Desktop)

## Attaching Cursor/VS Code to Docker Container

### Using Dev Containers (Recommended)

The project includes a `.devcontainer` configuration that allows you to work directly inside the Docker container using Cursor or VS Code.

**Steps:**

1. **Install Dev Containers extension** (if not already installed)
   - Open Extensions (Ctrl+Shift+X)
   - Search for "Dev Containers"
   - Install the Microsoft Dev Containers extension

2. **Reopen in Container**
   - Press `F1` or `Ctrl+Shift+P`
   - Type "Dev Containers: Reopen in Container"
   - Select it
   - Wait for the container to build and start (first time may take a few minutes)

3. **You're now inside the container!**
   - Terminal shows you're in `/workspace`
   - Rust tools are available
   - All dependencies are pre-installed
   - Code changes sync automatically

**Benefits:**
- Full IDE integration inside the container
- Rust Analyzer and debugging support
- Consistent environment across all machines
- No need to install Rust/Perl locally

**Alternative: Attach to Running Container**
- Start container: `docker-compose up -d`
- Press `F1` → "Dev Containers: Attach to Running Container"
- Select `sh2perl-test`

## Quick Start

### Build and Run Tests

```bash
# Build the Docker image
docker-compose build

# Run all tests
docker-compose run --rm test bash ./fail

# Run specific test
docker-compose run --rm test bash ./fail 000__03

# Run tests with Perl::Critic
docker-compose run --rm test bash ./fail --perl-critic
```

### Interactive Development

```bash
# Start an interactive container
docker-compose run --rm test bash

# Inside the container, you can:
# - Build the project
cargo build --bin debashc

# - Run tests
bash ./fail

# - Run specific tests
bash ./fail 044

# - Run Rust tests
cargo test

# - Run purify.pl tests
perl test_purify.pl --next --verbose
```

### One-off Commands

```bash
# Run a single command without entering the container
docker-compose run --rm test cargo test

# Run purify.pl help
docker-compose run --rm test perl purify.pl --help

# Check Perl modules
docker-compose run --rm test perl -MPPI -e "print 'PPI installed\n'"
docker-compose run --rm test perl -MPerl::Critic -e "print Perl::Critic->VERSION"
```

## Dockerfile Details

The Dockerfile sets up:

- **Rust toolchain** (latest stable)
- **Perl** with required modules:
  - PPI (Perl Parsing Interface)
  - PPI::Find
  - Perl::Critic
  - Getopt::Long
  - File::Basename
  - Time::HiRes
  - POSIX
- **Build tools** (gcc, make, etc.)
- **Git** for version control utilities
- **Bash** for running test scripts

## Volumes

The docker-compose.yml mounts:

- **Project directory** (`/workspace`) - for live code changes
- **Cargo cache** - to speed up rebuilds between container runs

## Environment Variables

The container sets:

- `LOCALE=C` - Consistent locale for tests
- `LC_COLLATE=C` - Consistent collation order
- `PATH` - Includes the built debashc binary

## Troubleshooting

### Rebuild from Scratch

```bash
# Remove old image and rebuild
docker-compose down
docker-compose build --no-cache
```

### Clear Cargo Cache

```bash
# Remove cargo cache volume
docker-compose down -v
```

### Check Container Status

```bash
# List running containers
docker ps

# View container logs
docker-compose logs test

# Execute command in running container
docker exec -it sh2perl-test bash
```

### Install Additional Perl Modules

If you need additional Perl modules, you can either:

1. **Modify Dockerfile** and rebuild:
   ```dockerfile
   RUN cpanm --notest Your::Module
   ```

2. **Install in running container**:
   ```bash
   docker-compose run --rm test cpanm Your::Module
   ```

### Permission Issues

If you encounter permission issues with mounted volumes:

```bash
# On Linux, you may need to adjust permissions
sudo chown -R $USER:$USER .
```

## CI/CD Integration

The Docker environment can be used in CI/CD pipelines:

```yaml
# Example GitHub Actions
- name: Run tests
  run: |
    docker-compose build
    docker-compose run --rm test bash ./fail
```

## Performance Tips

1. **Use volume mounts** - The docker-compose.yml already mounts the project directory, so code changes are immediately available
2. **Persist cargo cache** - The cargo cache volumes speed up rebuilds
3. **Build once, run many** - Build the image once, then run multiple test commands against it

## Comparison with Local Development

| Feature | Docker | Local |
|---------|--------|-------|
| Consistency | ✅ Same environment everywhere | ❌ Varies by OS/version |
| Isolation | ✅ Clean slate each time | ❌ System dependencies |
| Setup | ✅ One command | ❌ Multiple install steps |
| Performance | ⚠️ Slight overhead | ✅ Native speed |
| Debugging | ⚠️ Requires docker exec | ✅ Direct access |

Use Docker when:
- You want consistent test results across different machines
- You're setting up CI/CD
- You want to avoid polluting your system
- You're on Windows/Mac and want Linux-like behavior

Use local development when:
- You need maximum performance
- You're doing heavy debugging
- You prefer native tools

