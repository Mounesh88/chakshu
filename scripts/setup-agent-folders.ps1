# Prepares a working folder for each Chakshu agent, so the agents Switch starts can
# reach the read-only Chakshu MCP server.
#
#   powershell -ExecutionPolicy Bypass -File scripts\setup-agent-folders.ps1
#
# Why: Switch starts each agent with its own tool list and does not read your personal
# Claude Code settings. Claude Code does read project settings from the agent's own
# folder, so each folder gets:
#   .mcp.json              where the Chakshu MCP server is
#   .claude\settings.json  turns that server on and allows its read-only tools
#
# Run it again at any time; it only rewrites these two files.

param(
    [string]$Root = (Join-Path $HOME "chakshu-agents"),
    [string]$McpUrl = "http://127.0.0.1:8090/mcp"
)

$ErrorActionPreference = "Stop"
$agents = @("graph-analyst", "kyc-profiler", "cyber-signals", "case-writer")
# UTF-8 without a byte-order mark: Windows PowerShell adds one by default, and it breaks JSON.
$utf8 = New-Object System.Text.UTF8Encoding($false)

$mcp = '{"mcpServers":{"chakshu":{"type":"http","url":"' + $McpUrl + '"}}}'
$settings = '{"enableAllProjectMcpServers":true,"permissions":{"allow":["mcp__chakshu"]}}'

foreach ($agent in $agents) {
    $dir = Join-Path $Root $agent
    New-Item -ItemType Directory -Force -Path (Join-Path $dir ".claude") | Out-Null
    [IO.File]::WriteAllText((Join-Path $dir ".mcp.json"), $mcp, $utf8)
    [IO.File]::WriteAllText((Join-Path $dir ".claude\settings.json"), $settings, $utf8)
    Write-Host "Prepared $dir"
}

Write-Host ""
Write-Host "In Switch Console, when you import switch\chakshu-room.template.yaml, set the"
Write-Host "agents' directory (advanced options) to:"
Write-Host ""
Write-Host "    $(Join-Path $Root '{agent}')" -ForegroundColor Green
Write-Host ""
Write-Host "Keep the MCP server running: cargo run --release -p aml-mcp"
