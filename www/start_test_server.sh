#!/bin/bash
echo "Starting test results server..."
cd "$(dirname "$0")"
python3 test_results_server.py
