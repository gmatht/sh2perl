#!/bin/bash
# Quick start script for Docker test environment

set -e

echo "=== sh2perl Docker Test Environment ==="
echo ""

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    echo "Error: Docker is not running. Please start Docker and try again."
    exit 1
fi

echo "Building Docker image..."
docker-compose build

echo ""
echo "=== Docker Environment Ready ==="
echo ""
echo "Quick commands:"
echo "  Run all tests:        docker-compose run --rm test bash ./fail"
echo "  Run specific test:     docker-compose run --rm test bash ./fail TEST_PREFIX"
echo "  Interactive shell:     docker-compose run --rm test bash"
echo "  Verify setup:          docker-compose run --rm test bash ./test-docker.sh"
echo "  Run Rust tests:        docker-compose run --rm test cargo test"
echo ""
echo "For more information, see DOCKER.md"




