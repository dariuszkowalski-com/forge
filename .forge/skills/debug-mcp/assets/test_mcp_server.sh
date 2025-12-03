#!/bin/bash

# Test MCP Server for Debugging
# Creates a test MCP server that generates various error conditions

set -euo pipefail

# Configuration
TEST_SERVER_PORT=${1:-3001}
TEST_SERVER_TYPE=${2:-stdio}  # stdio or http

echo "=== MCP Test Server ==="
echo "Port: $TEST_SERVER_PORT"
echo "Type: $TEST_SERVER_TYPE"
echo "========================"

# Test 1: Valid MCP Server
echo "Test 1: Starting valid MCP server..."

if [[ "$TEST_SERVER_TYPE" == "http" ]]; then
    # HTTP test server
    cat > /tmp/test_mcp_http.js << 'EOF'
const http = require('http');

const server = http.createServer((req, res) => {
    res.setHeader('Content-Type', 'application/json');
    res.setHeader('Access-Control-Allow-Origin', '*');
    
    if (req.method === 'POST' && req.url === '/mcp') {
        let body = '';
        req.on('data', chunk => body += chunk.toString());
        req.on('end', () => {
            const request = JSON.parse(body);
            console.log('MCP Request:', JSON.stringify(request, null, 2));
            
            if (request.method === 'initialize') {
                res.writeHead(200);
                res.end(JSON.stringify({
                    jsonrpc: "2.0",
                    id: request.id,
                    result: {
                        protocolVersion: "2024-11-05",
                        capabilities: {
                            tools: {}
                        },
                        serverInfo: {
                            name: "test-mcp-server",
                            version: "1.0.0"
                        }
                    }
                }));
            } else if (request.method === 'tools/list') {
                res.writeHead(200);
                res.end(JSON.stringify({
                    jsonrpc: "2.0",
                    id: request.id,
                    result: {
                        tools: [
                            {
                                name: "test_tool",
                                description: "A test tool for debugging",
                                inputSchema: {
                                    type: "object",
                                    properties: {
                                        message: { type: "string" }
                                    }
                                }
                            }
                        ]
                    }
                }));
            }
        });
    } else {
        res.writeHead(200, {'Content-Type': 'text/plain'});
        res.end('MCP Test Server Running\n');
    }
});

server.listen(TEST_SERVER_PORT, () => {
    console.log(`Test MCP HTTP server listening on port ${TEST_SERVER_PORT}`);
});
EOF

    node /tmp/test_mcp_http.js &
    TEST_PID=$!
    echo "HTTP test server started with PID: $TEST_PID"
    echo "Test URL: http://localhost:$TEST_SERVER_PORT/mcp"
    
else
    # Stdio test server
    cat > /tmp/test_mcp_stdio.js << 'EOF'
// Test MCP stdio server
process.stdin.on('data', (data) => {
    const request = JSON.parse(data.toString());
    console.error('STDERR: Debug message from test MCP server');
    console.log('STDERR: This is stderr output that should be captured');
    
    if (request.method === 'initialize') {
        console.log(JSON.stringify({
            jsonrpc: "2.0",
            id: request.id,
            result: {
                protocolVersion: "2024-11-05",
                capabilities: { tools: {} },
                serverInfo: {
                    name: "test-mcp-server",
                    version: "1.0.0"
                }
            }
        }));
    } else if (request.method === 'tools/list') {
        console.log(JSON.stringify({
            jsonrpc: "2.0",
            id: request.id,
            result: {
                tools: [
                    {
                        name: "test_tool",
                        description: "A test tool for debugging",
                        inputSchema: {
                            type: "object",
                            properties: {
                                message: { type: "string" }
                            }
                        }
                    }
                ]
            }
        }));
    }
});

// Simulate some startup delay and stderr output
setTimeout(() => {
    console.error('STDERR: Server startup completed with warnings');
}, 1000);

console.error('STDERR: Test MCP server initializing...');
EOF

    node /tmp/test_mcp_stdio.js &
    TEST_PID=$!
    echo "Stdio test server started with PID: $TEST_PID"
fi

echo ""
echo "Test server is running. Press Enter to stop..."
read

# Cleanup
echo "Stopping test server..."
kill $TEST_PID 2>/dev/null || true
rm -f /tmp/test_mcp_*.js

echo "Test server stopped."

# Test 2: Invalid MCP Server (generates errors)
echo ""
echo "Test 2: Creating invalid MCP server configurations..."

# Test with non-existent command
cat > /tmp/invalid_mcp_config.json << 'EOF'
{
  "mcpServers": {
    "nonexistent-server": {
      "command": "nonexistent-command-that-does-not-exist",
      "args": []
    },
    "permission-denied": {
      "command": "/etc/passwd",
      "args": []
    },
    "syntax-error": {
      "command": "node",
      "args": ["-invalid-flag"]
    },
    "timeout-test": {
      "command": "sleep",
      "args": ["30"]
    }
  }
}
EOF

echo "Created invalid MCP config at: /tmp/invalid_mcp_config.json"
echo "Use this to test error handling:"
echo ""
echo "  # Test with invalid config"
echo "  FORGE_LOG=forge=debug forge mcp list --config /tmp/invalid_mcp_config.json"
echo ""
echo "  # Test with verbose output"
echo "  forge --verbose mcp list --config /tmp/invalid_mcp_config.json"

# Test 3: Network Issues
echo ""
echo "Test 3: Network connectivity tests..."

echo "Testing local network..."
if [[ "$TEST_SERVER_TYPE" == "http" ]]; then
    echo "HTTP test server should be accessible at: http://localhost:$TEST_SERVER_PORT"
    curl -v http://localhost:$TEST_SERVER_PORT/mcp || echo "HTTP connection failed"
fi

echo "Testing port availability..."
netstat -an | grep ":$TEST_SERVER_PORT " || echo "Port $TEST_SERVER_PORT is not in use"

echo "Testing DNS resolution..."
nslookup localhost || echo "DNS resolution failed"

echo ""
echo "=== Test Complete ==="
echo ""
echo "Next steps:"
echo "1. Test Forge with: forge mcp list"
echo "2. Enable debug logging: FORGE_LOG=forge=debug forge mcp list"
echo "3. Use verbose mode: forge --verbose mcp list"
echo "4. Check logs: tail -f ~/.forge/logs/forge.log"