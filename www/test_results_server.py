#!/usr/bin/env python3
"""
Simple HTTP server to serve test results for the web interface.
This allows the categories to be dynamic based on actual test outcomes.
"""

import json
import os
import sys
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse

class TestResultsHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        parsed_url = urlparse(self.path)
        
        if parsed_url.path == '/api/test-results':
            self.send_response(200)
            self.send_header('Content-type', 'application/json')
            self.send_header('Access-Control-Allow-Origin', '*')
            self.send_header('Access-Control-Allow-Methods', 'GET, OPTIONS')
            self.send_header('Access-Control-Allow-Headers', 'Content-Type')
            self.end_headers()
            
            test_results = self.get_test_results()
            self.wfile.write(json.dumps(test_results, indent=2).encode())
        else:
            self.send_response(404)
            self.end_headers()
            self.wfile.write(b'Not found')
    
    def do_OPTIONS(self):
        self.send_response(200)
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Methods', 'GET, OPTIONS')
        self.send_header('Access-Control-Allow-Headers', 'Content-Type')
        self.end_headers()
    
    def get_test_results(self):
        """Read test results from passed.txt and failed.txt files."""
        results = {
            'passed': [],
            'failed': []
        }
        
        # Read passed tests
        passed_file = '../passed.txt'
        if os.path.exists(passed_file):
            try:
                with open(passed_file, 'r') as f:
                    results['passed'] = [line.strip() for line in f if line.strip()]
            except Exception as e:
                print(f"Error reading {passed_file}: {e}")
        
        # Read failed tests
        failed_file = '../failed.txt'
        if os.path.exists(failed_file):
            try:
                with open(failed_file, 'r') as f:
                    results['failed'] = [line.strip() for line in f if line.strip()]
            except Exception as e:
                print(f"Error reading {failed_file}: {e}")
        
        return results
    
    def log_message(self, format, *args):
        # Suppress logging for cleaner output
        pass

def main():
    port = 8001  # Default port
    
    # Parse command line arguments
    if len(sys.argv) > 1:
        try:
            port = int(sys.argv[1])
        except ValueError:
            print(f"Invalid port number: {sys.argv[1]}")
            print("Usage: python test_results_server.py [port]")
            sys.exit(1)
    
    server_address = ('', port)
    httpd = HTTPServer(server_address, TestResultsHandler)
    
    print(f"Test results server running on http://localhost:{port}")
    print(f"API endpoint: http://localhost:{port}/api/test-results")
    print("Press Ctrl+C to stop")
    
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down server...")
        httpd.shutdown()

if __name__ == '__main__':
    main()
