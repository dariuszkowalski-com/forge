---
name: debug-mcp
description: Enhanced MCP debugging with verbose error output, stderr logging, and comprehensive diagnostic tools. Use when MCP servers fail to start or you need detailed error information to troubleshoot MCP connection issues.
---

# Debug MCP

This skill provides enhanced debugging capabilities for MCP (Model Context Protocol) servers in Forge. It implements two key improvements:

1. **Verbose error output** - Shows full error messages instead of truncated 80-character snippets
2. **MCP stderr logging** - Captures and logs stderr output from MCP servers

## Quick Start

### Apply Debug Patches
```bash
# Apply all debug improvements to your Forge installation
./scripts/apply_mcp_debug_patches.sh

# Or apply individual patches
./scripts/apply_mcp_debug_patches.sh --ui-only     # Option 1: Verbose UI
./scripts/apply_mcp_debug_patches.sh --stderr-only  # Option 2: Stderr logging
```

### Test Debug Features
```bash
# Test with verbose output (shows full errors)
forge --verbose mcp list

# Enable detailed logging
export FORGE_LOG=forge=debug
forge mcp list

# Check logs for MCP stderr output
tail -f ~/.forge/logs/forge.log | grep -i mcp
```

## Features

### Option 1: Verbose Error Output
- Shows complete error messages when using `--verbose` flag
- Eliminates 80-character truncation in UI
- Preserves full stack traces and diagnostic information

### Option 2: MCP Stderr Logging  
- Captures stderr output from stdio MCP servers
- Logs server startup errors and warnings
- Provides crucial debugging information for server failures

### HTTP Error Details
- Logs HTTP connection failures before SSE fallback
- Shows detailed network and protocol errors
- Helps diagnose remote MCP server issues

## Debugging Workflow

1. **Initial diagnosis**: `forge mcp list` (shows basic failures)
2. **Verbose details**: `forge --verbose mcp list` (shows full errors)  
3. **Deep logging**: `FORGE_LOG=forge=debug forge mcp list` (shows stderr + network)
4. **Log analysis**: Check `~/.forge/logs/forge.log` for MCP entries

## Common Issues

### "Failed to initiate MCP server" errors:
- **Cause**: Generic error message with truncated details
- **Solution**: Use `--verbose` flag to see full error

### MCP server won't start:
- **Cause**: Server stderr contains startup errors  
- **Solution**: Check logs with `FORGE_LOG=forge=debug`

### HTTP MCP connection failures:
- **Cause**: Network or authentication issues
- **Solution**: Enable debug logging to see HTTP error details

## Resources

- **[Complete Debug Guide](references/mcp_debug_guide.md)** - Comprehensive troubleshooting
- **[Test MCP Server](assets/test_mcp_server.sh)** - Test server for debugging
- **[Patches Directory](assets/patches/)** - Individual patch files

## Manual Implementation

If you prefer manual changes instead of using the script:

### UI Changes (Option 1)
Edit `crates/forge_main/src/ui.rs:1296-1302` to respect `--verbose` flag.

### Stderr Logging (Option 2)  
Edit `crates/forge_infra/src/mcp_client.rs:109-114` to capture and log stderr.

### HTTP Error Logging
Edit `crates/forge_infra/src/mcp_client.rs:123-135` to log HTTP failures.

## Testing

After applying patches, test with:
```bash
# Test verbose output
forge --verbose mcp list

# Test stderr capture  
forge mcp list 2>&1 | grep -i "mcp.*stderr"

# Verify logging
FORGE_LOG=debug forge mcp list
tail ~/.forge/logs/forge.log
```

## Reverting Changes

To remove debug improvements:
```bash
./scripts/apply_mcp_debug_patches.sh --revert
```

This will restore original Forge behavior with truncated errors and no stderr logging.