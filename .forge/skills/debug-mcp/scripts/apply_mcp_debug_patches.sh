#!/bin/bash

# Debug MCP Patches Application Script
# Applies debugging improvements to Forge MCP handling

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
FORGE_DIR="$(cd "$PROJECT_ROOT/.." && pwd)"

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warning() { echo -e "${YELLOW}[WARNING]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Help function
show_help() {
    cat << EOF
Debug MCP Patches Application Script

USAGE:
    $0 [OPTIONS]

OPTIONS:
    --ui-only       Apply only Option 1: Verbose UI error output
    --stderr-only    Apply only Option 2: MCP stderr logging  
    --http-only     Apply only HTTP error logging
    --revert        Revert all debug patches
    --help          Show this help message

EXAMPLES:
    $0                    # Apply all debug patches
    $0 --ui-only          # Apply only verbose UI improvements
    $0 --stderr-only       # Apply only stderr logging
    $0 --revert           # Remove all debug improvements
EOF
}

# Check if patches are already applied
is_patch_applied() {
    local patch_file="$1"
    local target_file="$2"
    local marker="$3"
    
    if [[ -f "$target_file" ]] && grep -q "$marker" "$target_file"; then
        return 0
    fi
    return 1
}

# Apply UI verbose patch (Option 1)
apply_ui_patch() {
    log_info "Applying UI verbose patch (Option 1)..."
    
    local ui_file="$FORGE_DIR/crates/forge_main/src/ui.rs"
    local patch_file="$SCRIPT_DIR/../assets/patches/verbose_ui.patch"
    local marker="MCP_VERBOSE_DEBUG"
    
    if is_patch_applied "$patch_file" "$ui_file" "$marker"; then
        log_warning "UI verbose patch already applied"
        return 0
    fi
    
    # Create the patch content
    cat > "$patch_file" << 'EOF'
--- a/crates/forge_main/src/ui.rs
+++ b/crates/forge_main/src/ui.rs
@@ -1293,8 +1293,15 @@
         // Show failed MCP servers
         if !all_tools.mcp.get_failures().is_empty() {
             info = info.add_title("FAILED");
             for (server_name, error) in all_tools.mcp.get_failures().iter() {
-                // Truncate error message for readability
-                let truncated_error = if error.len() > 80 {
-                    format!("{}...", &error[..77])
-                } else {
-                    error.clone()
-                };
+                // Show full error in verbose mode, truncated otherwise
+                let error_msg = if self.cli.verbose {
+                    error.clone()  // Full error in verbose mode
+                } else {
+                    // Truncate error message for readability in normal mode
+                    if error.len() > 80 {
+                        format!("{}...", &error[..77])
+                    } else {
+                        error.clone()
+                    }
+                };
-                info = info.add_value(format!("[✗] {server_name} - {truncated_error}"));
+                info = info.add_value(format!("[✗] {server_name} - {error_msg}"));
             }
         }
EOF
    
    # Apply the patch
    if (cd "$FORGE_DIR" && git apply "$patch_file"); then
        echo "// MCP_VERBOSE_DEBUG" >> "$ui_file"
        log_success "UI verbose patch applied successfully"
    else
        log_error "Failed to apply UI verbose patch"
        return 1
    fi
}

# Apply stderr logging patch (Option 2)
apply_stderr_patch() {
    log_info "Applying stderr logging patch (Option 2)..."
    
    local mcp_client_file="$FORGE_DIR/crates/forge_infra/src/mcp_client.rs"
    local patch_file="$SCRIPT_DIR/../assets/patches/stderr_logging.patch"
    local marker="MCP_STDERR_DEBUG"
    
    if is_patch_applied "$patch_file" "$mcp_client_file" "$marker"; then
        log_warning "Stderr logging patch already applied"
        return 0
    fi
    
    # Create the patch content
    cat > "$patch_file" << 'EOF'
--- a/crates/forge_infra/src/mcp_client.rs
+++ b/crates/forge_infra/src/mcp_client.rs
@@ -107,8 +107,20 @@
                 cmd.args(&stdio.args).kill_on_drop(true);
 
-                // Use builder pattern to capture and ignore stderr to silence MCP logs
-                let (transport, _stderr) = TokioChildProcess::builder(cmd)
-                    .stderr(std::process::Stdio::piped())
-                    .spawn()?;
+                // Capture stderr for debugging when verbose logging is enabled
+                let (transport, stderr) = TokioChildProcess::builder(cmd)
+                    .stderr(std::process::Stdio::piped())
+                    .spawn()?;
+                
+                // Log stderr in background task when debug logging is enabled
+                if let Some(mut stderr) = stderr {
+                    let server_name = stdio.command.clone();
+                    tokio::spawn(async move {
+                        use tokio::io::AsyncReadExt;
+                        let mut buffer = String::new();
+                        if let Ok(bytes_read) = stderr.read_to_string(&mut buffer).await {
+                            if bytes_read > 0 && !buffer.trim().is_empty() {
+                                tracing::warn!(
+                                    server = %server_name,
+                                    stderr = %buffer.trim(),
+                                    "MCP server stderr output"
+                                );
+                            }
+                        }
+                    });
+                }
+
                 self.client_info().serve(transport).await?
EOF
    
    # Apply the patch
    if (cd "$FORGE_DIR" && git apply "$patch_file"); then
        echo "// MCP_STDERR_DEBUG" >> "$mcp_client_file"
        log_success "Stderr logging patch applied successfully"
    else
        log_error "Failed to apply stderr logging patch"
        return 1
    fi
}

# Apply HTTP error logging patch
apply_http_patch() {
    log_info "Applying HTTP error logging patch..."
    
    local mcp_client_file="$FORGE_DIR/crates/forge_infra/src/mcp_client.rs"
    local patch_file="$SCRIPT_DIR/../assets/patches/http_errors.patch"
    local marker="MCP_HTTP_DEBUG"
    
    if is_patch_applied "$patch_file" "$mcp_client_file" "$marker"; then
        log_warning "HTTP error logging patch already applied"
        return 0
    fi
    
    # Create the patch content
    cat > "$patch_file" << 'EOF'
--- a/crates/forge_infra/src/mcp_client.rs
+++ b/crates/forge_infra/src/mcp_client.rs
@@ -122,8 +122,14 @@
                 match self.client_info().serve(transport).await {
                     Ok(client) => client,
-                    Err(_e) => {
+                    Err(e) => {
+                        tracing::warn!(
+                            url = %http.url,
+                            error = %e,
+                            "HTTP MCP connection failed, trying SSE fallback"
+                        );
+                        
                         let transport = SseClientTransport::start_with_client(
                             client,
                             SseClientConfig {
                                 sse_endpoint: http.url.clone().into(),
                                 ..Default::default()
                             },
                         )
                         .await?;
                         self.client_info().serve(transport).await?
                     }
                 }
EOF
    
    # Apply the patch
    if (cd "$FORGE_DIR" && git apply "$patch_file"); then
        echo "// MCP_HTTP_DEBUG" >> "$mcp_client_file"
        log_success "HTTP error logging patch applied successfully"
    else
        log_error "Failed to apply HTTP error logging patch"
        return 1
    fi
}

# Revert all patches
revert_patches() {
    log_info "Reverting all debug patches..."
    
    cd "$FORGE_DIR"
    
    # Remove debug markers from files
    local ui_file="$FORGE_DIR/crates/forge_main/src/ui.rs"
    local mcp_client_file="$FORGE_DIR/crates/forge_infra/src/mcp_client.rs"
    
    if [[ -f "$ui_file" ]]; then
        sed -i.bak '/\/\/ MCP_VERBOSE_DEBUG/d' "$ui_file" && rm -f "$ui_file.bak"
        log_success "Removed UI debug marker"
    fi
    
    if [[ -f "$mcp_client_file" ]]; then
        sed -i.bak '/\/\/ MCP_STDERR_DEBUG/d' "$mcp_client_file" && rm -f "$mcp_client_file.bak"
        sed -i.bak '/\/\/ MCP_HTTP_DEBUG/d' "$mcp_client_file" && rm -f "$mcp_client_file.bak"
        log_success "Removed MCP client debug markers"
    fi
    
    # Reset git changes
    if git checkout -- crates/forge_main/src/ui.rs crates/forge_infra/src/mcp_client.rs; then
        log_success "Reverted all patches successfully"
    else
        log_warning "Some patches could not be reverted automatically"
    fi
}

# Main execution
main() {
    case "${1:-}" in
        --ui-only)
            apply_ui_patch
            ;;
        --stderr-only)
            apply_stderr_patch
            ;;
        --http-only)
            apply_http_patch
            ;;
        --revert)
            revert_patches
            ;;
        --help|-h)
            show_help
            ;;
        "")
            log_info "Applying all debug patches..."
            apply_ui_patch
            apply_stderr_patch
            apply_http_patch
            log_success "All debug patches applied successfully!"
            log_info "Use 'forge --verbose mcp list' to see full error details"
            log_info "Use 'FORGE_LOG=forge=debug forge mcp list' to see stderr logging"
            ;;
        *)
            log_error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
}

# Check if we're in a Forge repository
if [[ ! -d "$FORGE_DIR/.git" ]]; then
    log_error "Not in a Forge repository. Please run this script from the Forge project directory."
    exit 1
fi

# Check if git is clean
if ! git -C "$FORGE_DIR" diff-index --quiet HEAD --; then
    log_warning "Working directory is not clean. Consider committing changes first."
fi

main "$@"