# Gemini-Extended UI/UX Feature Plan

Gemini-Extended is designed to take the powerful, autonomous capabilities of the terminal-based `gemini-cli` and wrap them in a stunning, native GTK4/Libadwaita interface. 

Here is the complete roadmap for mapping every `gemini-cli` feature into a beautiful GUI component.

## Phase 1: Core Chat & Sessions (✅ Completed)
- [x] Stream JSON parsing for real-time text output.
- [x] Workspace directory containment (`FileDialog`).
- [x] Session history list (`--list-sessions`).
- [x] Session resuming & JSON history parsing.
- [x] YOLO Mode toggle.
- [x] Material Design chat bubbles & Welcome Screen.
- [x] Dynamic Accent Color themes.

## Phase 2: Enhanced Artifact Cards (Tool Execution)
Currently, `ToolUse` events just show a basic card. We will upgrade these based on the tool executed:
- **`run_shell_command`:** Render a beautiful mini-terminal (using `vte` crate or a styled `TextView`) showing standard output. Add a loading spinner while the command runs.
- **`read_file` / `write_file`:** Show a mini code-editor block (`GtkSourceView`) with syntax highlighting showing the exact code Gemini modified.
- **`ask_user`:** When the CLI triggers an `ask_user` tool, dynamically render native GTK checkboxes or radio buttons inside the chat view, rather than text prompts.
- **`google_web_search` / `web_fetch`:** Render a rich link card with a favicon and page title when Gemini fetches a URL.

## Phase 3: Workspace & Project Settings
- **Extension Manager:** Add an "Extensions" tab in the Sidebar to visually toggle CLI extensions (like `frontend-design`, `rust-expert`). This will pass the `-e` flag.
- **Sandbox Mode:** A prominent lock icon toggle in the header. When enabled, passes `-s` to ensure Gemini cannot modify files outside the workspace.
- **Model Selection:** A dropdown in the `HeaderBar` to switch between `gemini-3.1-pro`, `gemini-3.1-flash`, etc.

## Phase 4: Developer QoL (Quality of Life) Features
- **Markdown Rendering:** Currently, text is raw. Integrate a Rust Markdown parser to render `<b>bold</b>`, links, and properly styled `<code>` blocks inside `bot-bubble`.
- **Syntax Highlighting:** Use `GtkSourceView` for all code blocks returned by Gemini. Add a "Copy Code" button to the top-right of every block.
- **Diff Viewer:** Before YOLO mode auto-approves a file change, render a visual Git-style Diff (Red/Green) asking the user "Approve Change?" (Matching `--approval-mode default`).
- **MCP Server Configuration:** A settings window dedicated to managing Model Context Protocol (MCP) servers, mapping to the `gemini mcp` CLI commands.