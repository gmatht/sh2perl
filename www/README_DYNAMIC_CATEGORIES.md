# Dynamic Categories System

This system automatically categorizes shell script examples based on the actual outcomes of equivalence tests, rather than using hardcoded categories.

## How It Works

1. **Test Results**: The system reads `passed.txt` and `failed.txt` files from the project root
2. **Dynamic Categorization**: Tests are automatically sorted into four categories:
   - **passed**: Tests that pass equivalence testing
   - **can parse**: Tests that can be parsed but fail execution
   - **can lex**: Tests that can be lexed but fail parsing
   - **failed**: Tests that fail at the lexing stage

3. **Real-time Updates**: Categories update automatically when test results change

## Setup

### Option 1: Start the Test Results Server

1. Navigate to the `www` directory
2. Start the server:
   - **Windows**: Double-click `start_test_server.bat` or run `start_test_server.ps1`
   - **Linux/Mac**: Run `python3 test_results_server.py`
3. The server will run on `http://localhost:8001`

### Option 2: Manual API Endpoint

If you have your own server, implement the `/api/test-results` endpoint that returns:

```json
{
  "passed": ["examples/001_simple.sh", "examples/005_args.sh"],
  "failed": ["examples/002_control_flow.sh", "examples/003_pipeline.sh"]
}
```

## Fallback Behavior

If the test results server is unavailable, the system falls back to static categories based on the last known test results.

## File Structure

- `examples.js` - Main examples file with dynamic categorization
- `test_results_server.py` - Python HTTP server for test results
- `start_test_server.bat` - Windows batch file to start server
- `start_test_server.ps1` - PowerShell script to start server

## Benefits

- **Accurate**: Categories reflect actual test outcomes
- **Maintainable**: No need to manually update categories
- **Real-time**: Categories update as tests are run
- **Robust**: Fallback to static categories if server unavailable

## Testing the System

1. Start the test results server
2. Open `index.html` in your browser
3. The categories should automatically populate based on current test results
4. Run new tests and refresh the page to see updated categories
