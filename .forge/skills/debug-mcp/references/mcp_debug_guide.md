# MCP Debug Guide

This guide provides comprehensive troubleshooting for MCP (Model Context Protocol) server issues in Forge.

## Table of Contents

1. [Quick Troubleshooting](#quick-troubleshooting)
2. [Understanding MCP Error Messages](#understanding-mcp-error-messages)
3. [Debug Logging Setup](#debug-logging-setup)
4. [Common MCP Issues](#common-mcp-issues)
5. [Advanced Debugging](#advanced-debugging)

## Quick Troubleshooting

### Basic Error Diagnosis
```bash
# Show basic MCP server status
forge mcp list

# Show full error messages (with debug patches applied)
forge --verbose mcp list

# Enable comprehensive logging
FORGE_LOG=forge=debug forge mcp list
```

### Log File Locations
```bash
# Find Forge log directory
forge info | grep "Logs"

# Real-time log monitoring
tail -f ~/.forge/logs/forge.log | grep -i mcp

# Search for specific server errors
grep -i "mcp.*server.*name" ~/.forge/logs/forge.log
```

## Understanding MCP Error Messages

### "Failed to initiate MCP server"
This generic error means the connection process failed. The actual cause is in the detailed error message.

**Without debug patches**: Only shows server name
```bash
[✗] my-mcp-server - Failed to initiate MCP server: my-mcp-server
```

**With debug patches + --verbose**: Shows full error details
```bash
[✗] my-mcp-server - Failed to initiate MCP server: my-mcp-server: Command failed: No such file or directory (os error 2)
```

### Common Error Patterns

| Error Pattern | Cause | Solution |
|--------------|--------|----------|
| `No such file or directory` | Command doesn't exist | Check command path in MCP config |
| `Permission denied` | Command not executable | Add execute permissions or use `node` |
| `Connection refused` | Server not running | Start MCP server or check port |
| `Timeout` | Server slow to respond | Increase timeout or check server health |
| `Authentication failed` | Invalid credentials | Check API keys/headers |

## Debug Logging Setup

### Enable Detailed Logging
```bash
# Method 1: Environment variable
export FORGE_LOG=forge=debug

# Method 2: One-time command
FORGE_LOG=forge=debug forge mcp list

# Method 3: Persistent setting
echo 'export FORGE_LOG=forge=debug' >> ~/.bashrc
source ~/.bashrc
```

### Log Levels for MCP
- `forge=error`: Only critical errors
- `forge=warn`: Warnings and errors  
- `forge=info`: General information
- `forge=debug`: Full debugging (includes stderr)
- `forge=trace`: Maximum detail (recommended for MCP issues)

### Log Analysis Commands
```bash
# Show recent MCP errors
tail -100 ~/.forge/logs/forge.log | grep -i "error.*mcp"

# Show MCP server startup
grep -i "mcp.*server.*stderr" ~/.forge/logs/forge.log

# Filter by specific server
grep -i "mcp.*server.*my-server" ~/.forge/logs/forge.log

# Show HTTP connection issues
grep -i "http.*mcp.*connection" ~/.forge/logs/forge.log
```

## Common MCP Issues

### 1. Stdio MCP Server Won't Start

**Symptoms**:
- "Failed to initiate MCP server" with no details
- Server appears in config but not in list

**Debugging Steps**:
```bash
# 1. Check if command exists
which npx  # or node, python, etc.

# 2. Test command manually
npx @modelcontextprotocol/server-filesystem /tmp

# 3. Check with debug logging
FORGE_LOG=forge=debug forge mcp list
grep -i "stderr.*mcp" ~/.forge/logs/forge.log
```

**Common Solutions**:
- Install missing dependencies: `npm install -g @modelcontextprotocol/server-filesystem`
- Use absolute paths in MCP config
- Add environment variables to MCP config

### 2. HTTP MCP Connection Failures

**Symptoms**:
- Connection timeouts
- Authentication errors
- SSL/TLS issues

**Debugging Steps**:
```bash
# Test HTTP connection manually
curl -v https://my-mcp-server.com/mcp

# Check with Forge debug logging
FORGE_LOG=forge=debug forge mcp list
grep -i "http.*mcp" ~/.forge/logs/forge.log
```

**Common Solutions**:
- Verify URL accessibility
- Check authentication headers
- Update SSL certificates
- Configure proxy settings

### 3. MCP Server Starts but No Tools

**Symptoms**:
- Server connects successfully
- No tools listed in output

**Debugging Steps**:
```bash
# Check server tools manually
curl -X POST http://localhost:3000/tools/list

# Verify MCP protocol version
FORGE_LOG=forge=debug forge mcp list
grep -i "tools.*list" ~/.forge/logs/forge.log
```

## Advanced Debugging

### Manual MCP Server Testing

Use the test server to isolate issues:
```bash
# Start test server
./assets/test_mcp_server.sh

# Test connection in another terminal
forge mcp list
```

### MCP Configuration Validation

```bash
# Validate MCP config syntax
cat ~/.forge/.mcp.json | jq .

# Check specific server config
forge mcp show my-server

# Test with minimal config
cp ~/.forge/.mcp.json ~/.forge/.mcp.json.backup
echo '{"mcpServers": {}}' > ~/.forge/.mcp.json
forge mcp list
```

### Network Debugging

```bash
# Monitor network connections
netstat -an | grep :3000

# Test with different endpoints
curl -I https://my-mcp-server.com/health
curl -X POST https://my-mcp-server.com/mcp -d '{"jsonrpc":"2.0","id":1,"method":"initialize"}'

# Check DNS resolution
nslookup my-mcp-server.com
```

### Process Debugging

```bash
# Check if MCP server process is running
ps aux | grep mcp

# Monitor process activity
htop | grep mcp

# Check process logs
journalctl -u my-mcp-service -f
```

## Performance Issues

### Slow MCP Server Responses

**Symptoms**:
- Long delays in `forge mcp list`
- Timeout errors

**Solutions**:
```bash
# Increase timeout in MCP config (if supported)
{
  "mcpServers": {
    "my-server": {
      "timeout": 30000  // 30 seconds
    }
  }
}

# Monitor performance
time forge mcp list

# Check server resources
top | grep mcp-server
```

### Memory/CPU Issues

```bash
# Monitor resource usage
watch -n 1 'ps aux | grep mcp'

# Check Forge memory usage
ps aux | grep forge

# Restart Forge if memory leak suspected
pkill forge
forge mcp list
```

## Recovery Procedures

### Reset MCP Configuration

```bash
# Backup current config
cp ~/.forge/.mcp.json ~/.forge/.mcp.json.backup

# Reset to default
echo '{"mcpServers": {}}' > ~/.forge/.mcp.json

# Test with clean config
forge mcp list

# Restore if needed
cp ~/.forge/.mcp.json.backup ~/.forge/.mcp.json
```

### Clear MCP Cache

```bash
# Find cache location (implementation dependent)
find ~/.forge -name "*mcp*cache*" -type d

# Clear cache (force reconnection)
rm -rf ~/.forge/cache/mcp
forge mcp list
```

### Revert Debug Changes

```bash
# Remove debug patches
./scripts/apply_mcp_debug_patches.sh --revert

# Restart Forge to ensure clean state
pkill forge
forge mcp list
```

## Getting Help

### Collect Debug Information

When seeking help, collect this information:

```bash
# System info
forge info

# MCP configuration
forge mcp list --porcelain

# Recent logs
tail -200 ~/.forge/logs/forge.log

# Network status
ping -c 3 my-mcp-server.com
```

### Common Support Commands

```bash
# Show Forge version
forge --version

# Check Rust toolchain
rustc --version
cargo --version

# Verify MCP protocol implementation
find ~/.forge -name "*.json" -exec echo "File: {}" \; -exec cat {} \;
```

This guide should help resolve most MCP server issues. If problems persist, the collected debug information will be valuable for further assistance.