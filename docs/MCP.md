# Glimpse MCP Integration

Glimpse ships an embedded [Model Context Protocol](https://modelcontextprotocol.io) server, letting AI agents read text from any pixel on your screen.

## Wire it up

### Claude Code

Add to `~/.claude/settings.json` under `mcpServers`:

```json
{
  "mcpServers": {
    "glimpse": {
      "command": "glimpse",
      "args": ["mcp"]
    }
  }
}
```

Restart Claude Code. The Glimpse tools will appear in the agent's tool catalog.

### Cursor

Add to `~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "glimpse": {
      "command": "glimpse",
      "args": ["mcp"]
    }
  }
}
```

### Cline / Continue / OpenCode

Same pattern — point at `glimpse mcp` as a stdio MCP server.

## Tools exposed

### `ocr_at_cursor()`

Captures a DPI-aware region around the current mouse cursor and returns the OCR'd text.

```jsonc
// Returns:
{ "text": "Lorem ipsum dolor sit amet" }
```

### `ocr_region(x, y, width, height)`

Captures an arbitrary screen region (virtual-desktop coordinates) and returns the OCR'd text.

```jsonc
// Args: { "x": 100, "y": 200, "width": 400, "height": 200 }
// Returns:
{ "text": "..." }
```

### `read_clipboard()`

Returns the current clipboard text (UTF-8).

```jsonc
// Returns:
{ "text": "..." }
```

## Permissions

On the first MCP tool call from a given agent session, Glimpse shows a system permission prompt:

```
[Agent name] wants to read your screen via Glimpse.

[ Allow once ]  [ Allow this session ]  [ Deny ]
```

Subsequent calls in the same session use the chosen scope. The Glimpse tray icon flashes briefly on every capture so the action is never silent.

You can revoke an agent's session permission at any time from the tray menu → "Active agent sessions."

## Common workflows

- **"Read what's in this YouTube video frame and translate it"** — the agent calls `ocr_at_cursor()` while you hover over the video.
- **"Copy all the error messages from this game's console"** — agent calls `ocr_region()` with the console's bounds.
- **"What did I just copy?"** — agent calls `read_clipboard()` to ground its next response.

## Limits

- Glimpse only reads pixels it can see. Content behind another window, on a different virtual desktop, or inside a protected DRM surface (e.g. Netflix in some browsers, some banking apps) cannot be captured.
- OCR accuracy depends on the Windows OCR API. Stylized fonts, low-contrast text, and handwritten content may produce errors. A future `paddle_ocr_at_cursor()` tool (v2) will offer higher accuracy at the cost of latency.
