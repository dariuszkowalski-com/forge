use std::borrow::Cow;
use std::collections::BTreeMap;
use std::future::Future;
use std::str::FromStr;
use std::sync::{Arc, OnceLock, RwLock};

use backon::{ExponentialBuilder, Retryable};
use forge_app::McpClientInfra;
use forge_domain::{Image, McpHttpServer, McpServerConfig, ToolDefinition, ToolName, ToolOutput};
use http::{HeaderName, HeaderValue, header};
use rmcp::model::{CallToolRequestParam, ClientInfo, Implementation, InitializeRequestParam};
use rmcp::service::RunningService;
use rmcp::transport::sse_client::SseClientConfig;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{SseClientTransport, StreamableHttpClientTransport, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use schemars::schema::RootSchema;
use serde_json::Value;
use tokio::process::Command;

use crate::error::Error;

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

type RmcpClient = RunningService<RoleClient, InitializeRequestParam>;

#[derive(Clone)]
pub struct ForgeMcpClient {
    client: Arc<RwLock<Option<Arc<RmcpClient>>>>,
    config: McpServerConfig,
    env_vars: BTreeMap<String, String>,
    resolved_config: Arc<OnceLock<anyhow::Result<McpServerConfig>>>,
}

impl ForgeMcpClient {
    pub fn new(config: McpServerConfig, env_vars: &BTreeMap<String, String>) -> Self {
        Self {
            client: Default::default(),
            config,
            env_vars: env_vars.clone(),
            resolved_config: Arc::new(OnceLock::new()),
        }
    }

    /// Gets the resolved configuration, lazily initializing templates if needed
    fn get_resolved_config(&self) -> anyhow::Result<&McpServerConfig> {
        self.resolved_config
            .get_or_init(|| match &self.config {
                McpServerConfig::Http(http) => {
                    resolve_http_templates(http.clone(), &self.env_vars).map(McpServerConfig::Http)
                }
                x => Ok(x.clone()),
            })
            .as_ref()
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    fn client_info(&self) -> ClientInfo {
        ClientInfo {
            protocol_version: Default::default(),
            capabilities: Default::default(),
            client_info: Implementation {
                name: "Forge".to_string(),
                version: VERSION.to_string(),
                icons: None,
                title: None,
                website_url: None,
            },
        }
    }

    /// Connects to the MCP server. If `force` is true, it will reconnect even
    /// if already connected.
    async fn connect(&self) -> anyhow::Result<Arc<RmcpClient>> {
        if let Some(client) = self.get_client() {
            Ok(client.clone())
        } else {
            let client = self.create_connection().await?;
            self.set_client(client.clone());
            Ok(client.clone())
        }
    }

    fn get_client(&self) -> Option<Arc<RmcpClient>> {
        self.client.read().ok().and_then(|guard| guard.clone())
    }

    fn set_client(&self, client: Arc<RmcpClient>) {
        if let Ok(mut guard) = self.client.write() {
            *guard = Some(client);
        }
    }

    async fn create_connection(&self) -> anyhow::Result<Arc<RmcpClient>> {
        let config = self.get_resolved_config()?;
        
        // Check for ZAI servers and use proxy
        if let forge_domain::McpServerConfig::Http(http) = &config {
            if http.url.contains("api.z.ai") {
                println!("🚀 ZAI server detected in mcp_client.rs, attempting proxy...");
                return self.create_zai_proxy_connection(http).await;
            }
        }
        
        let client = match config {
            McpServerConfig::Stdio(stdio) => {
                let mut cmd = Command::new(stdio.command.clone());

                for (key, value) in &stdio.env {
                    cmd.env(key, value);
                }

                cmd.args(&stdio.args).kill_on_drop(true);

                // Capture stderr for debugging when verbose logging is enabled
                let (transport, stderr) = TokioChildProcess::builder(cmd)
                    .stderr(std::process::Stdio::piped())
                    .spawn()?;
                
                // Log stderr in background task when debug logging is enabled
                if let Some(mut stderr) = stderr {
                    let server_name = stdio.command.clone();
                    tokio::spawn(async move {
                        use tokio::io::AsyncReadExt;
                        let mut buffer = String::new();
                        if let Ok(bytes_read) = stderr.read_to_string(&mut buffer).await {
                            if bytes_read > 0 && !buffer.trim().is_empty() {
                                tracing::error!(
                                    server = %server_name,
                                    stderr = %buffer.trim(),
                                    "MCP server stderr output"
                                );
                            }
                        }
                    });
                }

                self.client_info().serve(transport).await?
            }
            McpServerConfig::Http(http) => {
                // Try HTTP first, fall back to SSE if it fails
                let client = self.reqwest_client(http)?;
                let transport = StreamableHttpClientTransport::with_client(
                    client.clone(),
                    StreamableHttpClientTransportConfig::with_uri(http.url.clone()),
                );
                match self.client_info().serve(transport).await {
                    Ok(client) => client,
                    Err(_e) => {
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
            }
        };

        Ok(Arc::new(client))
    }

    /// Create ZAI proxy connection using built-in Forge proxy
    async fn create_zai_proxy_connection(&self, http: &McpHttpServer) -> anyhow::Result<Arc<RmcpClient>> {
        tracing::info!("🚀 Using Forge built-in ZAI MCP proxy for: {}", http.url);
        
        // Extract model name from URL
        let model_name = if http.url.contains("web_reader") {
            "zai/web-reader"
        } else if http.url.contains("web_search_prime") {
            "zai/web-search-prime"
        } else {
            return Err(anyhow::anyhow!("Unsupported ZAI server URL: {}", http.url));
        };

        // Extract API key from Authorization header
        let auth_header = http.headers
            .get("Authorization")
            .ok_or_else(|| anyhow::anyhow!("ZAI server requires Authorization header"))?;
            
        // Remove "Bearer " prefix to get the actual API key
        let api_key = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| anyhow::anyhow!("ZAI server requires Bearer token in Authorization header"))?;

        // Create a simple proxy command that acts as MCP server
        // We'll use a simple Node.js script that forwards requests to ZAI API
        let proxy_script = format!(r#"
// Forge ZAI MCP Proxy Server
const http = require('http');
const https = require('https');

const MODEL = "{}";
const API_KEY = "{}";

// ZAI API endpoints
const ZAI_ENDPOINTS = {{
    "zai/web-reader": "https://api.z.ai/api/mcp/web_reader/mcp",
    "zai/web-search-prime": "https://api.z.ai/api/mcp/web_search_prime/mcp"
}};

const ENDPOINT = ZAI_ENDPOINTS[MODEL];
if (!ENDPOINT) {{
    console.error('Unsupported ZAI model:', MODEL);
    process.exit(1);
}}

// Simple MCP server that forwards to ZAI API
const server = http.createServer((req, res) => {{
    let body = '';
    
    req.on('data', chunk => {{
        body += chunk.toString();
    }});
    
    req.on('end', () => {{
        try {{
            const mcpRequest = JSON.parse(body);
            
            // Forward request to ZAI API
            const zaiReq = https.request(ENDPOINT, {{
                method: 'POST',
                headers: {{
                    'Content-Type': 'application/json',
                    'Authorization': 'Bearer ' + API_KEY,
                    'User-Agent': 'Forge-ZAI-Proxy/1.0'
                }}
            }});
            
            zaiReq.on('response', (zAIRes) => {{
                let zaiBody = '';
                
                ZAIRes.on('data', chunk => {{
                    zaiBody += chunk.toString();
                }});
                
                ZAIRes.on('end', () => {{
                    // Forward ZAI response back to MCP client
                    res.writeHead(ZAIRes.statusCode, {{
                        'Content-Type': 'application/json',
                        'Access-Control-Allow-Origin': '*'
                    }});
                    res.end(zaiBody);
                }});
            }});
            
            zaiReq.on('error', (error) => {{
                console.error('ZAI API error:', error);
                res.writeHead(500, {{ 'Content-Type': 'application/json' }});
                res.end(JSON.stringify({{
                    jsonrpc: "2.0",
                    id: mcpRequest.id || 1,
                    error: {{
                        code: -32603,
                        message: "ZAI API error: " + error.message
                    }}
                }}));
            }});
            
            zaiReq.write(body);
            zaiReq.end();
            
        }} catch (error) {{
            console.error('Proxy error:', error);
            res.writeHead(400, {{ 'Content-Type': 'application/json' }});
            res.end(JSON.stringify({{
                jsonrpc: "2.0",
                id: 1,
                error: {{
                    code: -32700,
                    message: "Invalid MCP request: " + error.message
                }}
            }}));
        }}
    }});
    
    req.on('error', (error) => {{
        console.error('Request error:', error);
        res.writeHead(500);
        res.end('Internal Server Error');
    }});
}});

// Start proxy server on random port
server.listen(0, () => {{
    const port = server.address().port;
    console.log(JSON.stringify({{
        jsonrpc: "2.0",
        id: 1,
        result: {{
            server_url: `http://localhost:${{port}}`,
            proxy_info: "Forge ZAI MCP Proxy for " + MODEL
        }}
    }}));
}});
"#, model_name, api_key);

        // Create temporary proxy script
        let proxy_script_path = format!("/tmp/forge_zai_proxy_{}.js", std::process::id());
        std::fs::write(&proxy_script_path, proxy_script)?;
        
        // Start proxy server
        let mut cmd = Command::new("node");
        cmd.arg(&proxy_script_path);
        cmd.kill_on_drop(true);

        // Capture stdout to get server URL
        let (transport, stdout) = TokioChildProcess::builder(cmd)
            .stdout(std::process::Stdio::piped())
            .spawn()?;
        
        // Read the server URL from stdout
        if let Some(mut stdout) = stdout {
            use tokio::io::AsyncReadExt;
            let mut buffer = String::new();
            if let Ok(bytes_read) = stdout.read_to_string(&mut buffer).await {
                if bytes_read > 0 {
                    tracing::debug!("ZAI proxy stdout: {}", buffer.trim());
                    
                    // Parse the server URL from the proxy output
                    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&buffer) {
                        if let Some(server_url) = response.get("result")
                            .and_then(|r| r.get("server_url"))
                            .and_then(|u| u.as_str()) {
                            
                            // Connect to the proxy server
                            let proxy_client = self.reqwest_client(&McpHttpServer {
                                url: server_url.to_string(),
                                headers: std::collections::BTreeMap::new(),
                                disable: false,
                            })?;
                            
                            let transport = StreamableHttpClientTransport::with_client(
                                proxy_client,
                                StreamableHttpClientTransportConfig::with_uri(server_url.to_string()),
                            );
                            
                            let client = self.client_info().serve(transport).await?;
                            tracing::info!("✅ ZAI MCP proxy connected successfully for model: {}", model_name);
                            
                            // Clean up temporary script
                            let _ = std::fs::remove_file(&proxy_script_path);
                            
                            return Ok(Arc::new(client));
                        }
                    }
                }
            }
        }
        
        Err(anyhow::anyhow!("Failed to start ZAI proxy server"))
    }

    fn reqwest_client(&self, config: &McpHttpServer) -> anyhow::Result<reqwest::Client> {
        let mut headers = header::HeaderMap::new();
        for (key, value) in config.headers.iter() {
            headers.insert(HeaderName::from_str(key)?, HeaderValue::from_str(value)?);
        }

        let client = reqwest::Client::builder().default_headers(headers);
        Ok(client.build()?)
    }

    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        let client = self.connect().await?;
        let tools = client.list_tools(None).await?;
        Ok(tools
            .tools
            .into_iter()
            .filter_map(|tool| {
                Some(
                    ToolDefinition::new(tool.name)
                        .description(tool.description.unwrap_or_default())
                        .input_schema(
                            serde_json::from_value::<RootSchema>(Value::Object(
                                tool.input_schema.as_ref().clone(),
                            ))
                            .ok()?,
                        ),
                )
            })
            .collect())
    }

    async fn call(&self, tool_name: &ToolName, input: &Value) -> anyhow::Result<ToolOutput> {
        let client = self.connect().await?;
        let result = client
            .call_tool(CallToolRequestParam {
                name: Cow::Owned(tool_name.to_string()),
                arguments: if let Value::Object(args) = input {
                    Some(args.clone())
                } else {
                    None
                },
            })
            .await?;

        let tool_contents: Vec<ToolOutput> = result
            .content
            .into_iter()
            .map(|content| match content.raw {
                rmcp::model::RawContent::Text(raw_text_content) => {
                    Ok(ToolOutput::text(raw_text_content.text))
                }
                rmcp::model::RawContent::Image(raw_image_content) => Ok(ToolOutput::image(
                    Image::new_base64(raw_image_content.data, raw_image_content.mime_type.as_str()),
                )),
                rmcp::model::RawContent::Resource(_) => {
                    Err(Error::UnsupportedMcpResponse("Resource").into())
                }
                rmcp::model::RawContent::ResourceLink(_) => {
                    Err(Error::UnsupportedMcpResponse("ResourceLink").into())
                }
                rmcp::model::RawContent::Audio(_) => {
                    Err(Error::UnsupportedMcpResponse("Audio").into())
                }
            })
            .collect::<anyhow::Result<Vec<ToolOutput>>>()?;

        Ok(ToolOutput::from(tool_contents.into_iter())
            .is_error(result.is_error.unwrap_or_default()))
    }

    async fn attempt_with_retry<T, F>(&self, call: impl Fn() -> F) -> anyhow::Result<T>
    where
        F: Future<Output = anyhow::Result<T>>,
    {
        call.retry(
            ExponentialBuilder::default()
                .with_max_times(5)
                .with_jitter(),
        )
        .when(|err| {
            let is_transport = err
                .downcast_ref::<rmcp::ServiceError>()
                .map(|e| {
                    matches!(
                        e,
                        rmcp::ServiceError::TransportSend(_) | rmcp::ServiceError::TransportClosed
                    )
                })
                .unwrap_or(false);

            if is_transport && let Ok(mut guard) = self.client.write() {
                guard.take();
            }

            is_transport
        })
        .await
    }
}

#[async_trait::async_trait]
impl McpClientInfra for ForgeMcpClient {
    async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
        self.attempt_with_retry(|| self.list()).await
    }

    async fn call(&self, tool_name: &ToolName, input: Value) -> anyhow::Result<ToolOutput> {
        self.attempt_with_retry(|| self.call(tool_name, &input))
            .await
    }
}

/// Resolves mustache templates in McpHttpServer headers using Handlebars
/// and provided environment variables
fn resolve_http_templates(
    mut http: McpHttpServer,
    env_vars: &BTreeMap<String, String>,
) -> anyhow::Result<McpHttpServer> {
    let handlebars = forge_app::TemplateEngine::handlebar_instance();

    // Create template data with env variables nested under "env"
    let template_data = serde_json::json!({"env": env_vars});

    // Resolve templates in headers
    for (_, value) in http.headers.iter_mut() {
        // Try to render the template, but keep original value if it fails
        if let Ok(resolved) = handlebars.render_template(value, &template_data) {
            *value = resolved;
        }
    }

    Ok(http)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_resolve_http_templates_with_env() {
        let env_vars = BTreeMap::from([
            ("GH_TOKEN".to_string(), "secret_token_123".to_string()),
            ("API_KEY".to_string(), "api_key_456".to_string()),
        ]);

        let http = McpHttpServer {
            url: "https://api.example.com".to_string(),
            headers: BTreeMap::from([
                (
                    "Authorization".to_string(),
                    "Bearer {{env.GH_TOKEN}}".to_string(),
                ),
                ("X-API-Key".to_string(), "{{env.API_KEY}}".to_string()),
                ("Content-Type".to_string(), "application/json".to_string()),
            ]),
            disable: false,
        };

        let resolved = resolve_http_templates(http, &env_vars).unwrap();

        assert_eq!(
            resolved.headers.get("Authorization"),
            Some(&"Bearer secret_token_123".to_string())
        );
        assert_eq!(
            resolved.headers.get("X-API-Key"),
            Some(&"api_key_456".to_string())
        );
        assert_eq!(
            resolved.headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn test_resolve_http_templates_missing_env_var() {
        let env_vars = BTreeMap::new(); // Empty env vars

        let http = McpHttpServer {
            url: "https://api.example.com".to_string(),
            headers: BTreeMap::from([(
                "Authorization".to_string(),
                "Bearer {{env.MISSING_VAR}}".to_string(),
            )]),
            disable: false,
        };

        let resolved = resolve_http_templates(http, &env_vars).unwrap();

        // Should keep original value if template rendering fails
        assert_eq!(
            resolved.headers.get("Authorization"),
            Some(&"Bearer {{env.MISSING_VAR}}".to_string())
        );
    }

    #[test]
    fn test_resolve_http_templates_preserves_url_and_disable() {
        let env_vars = BTreeMap::from([("TOKEN".to_string(), "test".to_string())]);

        let http = McpHttpServer {
            url: "https://test.example.com".to_string(),
            headers: BTreeMap::from([("Auth".to_string(), "{{env.TOKEN}}".to_string())]),
            disable: true,
        };

        let resolved = resolve_http_templates(http, &env_vars).unwrap();

        assert_eq!(resolved.url, "https://test.example.com");
        assert_eq!(resolved.disable, true);
        assert_eq!(resolved.headers.get("Auth"), Some(&"test".to_string()));
    }
}
