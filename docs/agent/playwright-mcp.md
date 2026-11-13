# Playwright MCP (CDP)

Browser automation for Ajax agents via `@playwright/mcp` and host Chrome.

## Why Chrome runs on the host

MCP attaches to host Chrome on CDP. Agent containers use host networking, so
`127.0.0.1:9222` stays reachable from them. The repo wrapper is
`scripts/playwright-mcp-cdp`:

```bash
install -m 755 /path/to/ajax-cli/scripts/playwright-mcp-cdp ~/.local/bin/playwright-mcp-cdp
```

Install under `~/.local/bin` because task worktrees are reaped; MCP config must not
point at a worktree path.

## Headless Chrome LaunchAgent

launchd does not expand environment variables in `ProgramArguments` or
`StandardErrorPath`. Save the template below with literal `$HOME` placeholders, then
run this from a host shell so Chrome gets real paths:

```bash
sed -i "" "s|\$HOME|$HOME|g" ~/Library/LaunchAgents/com.ajax.playwright-chrome.plist
```

Adjust the Chrome binary path in the plist if needed. Create the profile parent
directory before bootstrapping:

```bash
mkdir -p ~/.local/share/playwright-chrome
```

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.ajax.playwright-chrome</string>
  <key>ProgramArguments</key>
  <array>
    <string>/Applications/Google Chrome.app/Contents/MacOS/Google Chrome</string>
    <string>--headless=new</string>
    <string>--remote-debugging-port=9222</string>
    <string>--user-data-dir=$HOME/.local/share/playwright-chrome/profile</string>
    <string>--no-first-run</string>
    <string>--no-default-browser-check</string>
    <string>--use-mock-keychain</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardErrorPath</key>
  <string>$HOME/.local/share/playwright-chrome/chrome.log</string>
</dict>
</plist>
```

Save as `~/Library/LaunchAgents/com.ajax.playwright-chrome.plist`, run the `sed` command
above, then run
`launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.ajax.playwright-chrome.plist`
from a host shell. Agents run in a Linux container and cannot run `launchctl`.
Stop with `launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.ajax.playwright-chrome.plist`.

## Harness registration

```bash
claude mcp add -s user playwright -- ~/.local/bin/playwright-mcp-cdp
codex mcp add playwright -- ~/.local/bin/playwright-mcp-cdp
pi mcp add playwright -- ~/.local/bin/playwright-mcp-cdp
```

Cursor: in `~/.cursor/mcp.json`, add `playwright` under `mcpServers` with `command`
set to the absolute path of `~/.local/bin/playwright-mcp-cdp`.

## Security

Port 9222 CDP has no authentication. Any local process can control Chrome. An agent
driving MCP steers host Chrome, including `file://` reads of any host path and
download redirection to arbitrary paths (known protocol behavior; not tested here).
Playwright MCP is not a security boundary. Use `launchctl bootout` when not doing
browser work.

## Verification status

Navigate, type, and click verified for Claude Code, Cursor, and Pi. Codex is registered
but not exercised.
