# Sentinel App Server & IDE Companion Protocol Specification

## JSON-RPC 2.0 API Specification

The Sentinel App Server exposes a JSON-RPC 2.0 interface over TCP and stdio transports.

### Session Methods
- `session/create`: Initialize a new agent session.
- `session/destroy`: Destroy an active session.
- `session/get`: Get session metadata.

### Conversation Methods
- `chat`: Send a user prompt and return the assistant response.
- `chat/stream`: Stream assistant response chunks and tool call events.
- `chat/getHistory`: Fetch complete turn history for a session.

### Interactive Form & Dialog Methods
- `dialog/askUser`: Emitted by server when the agent requires form input or radio selection from user.
- `dialog/submitResponse`: Sent by client to return the user's form response.

### Session Browser Methods
- `session/browserList`: Returns summary records (`id`, `title`, `created_at`, `total_tokens`, `message_count`) for history inspection.

### IDE Companion Methods
- `ide/contextSync`: Pushes active document path, open tab list, cursor line/col, and text selection from VS Code / IDE.
- `ide/diffPreview`: Pushes red/green line-by-line diff previews to the IDE editor.
