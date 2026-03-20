# Usage

## Supported Markdown

`jetmd` renders standard Markdown and also enables:

- tables
- task lists
- strikethrough

Common elements such as headings, emphasis, links, images, blockquotes, code blocks, inline code, ordered and unordered lists, and horizontal rules are supported.

## Everyday workflow

- Use the **Open** button or `Ctrl+O` to open `.md`, `.markdown`, `.txt`, or other UTF-8 text files.
- Use `Ctrl+N` to create a new tab.
- Use the recent files menu to reopen previously accessed documents.
- Use **Save** or **Save As** to write Markdown files back to disk.
- Use **Export as HTML** to generate a standalone HTML document from the active tab.
- Use **Print** from the hamburger menu to print the rendered preview or save it as a PDF via the system print dialog.

## Preview themes

The preview pane ships with a set of built-in themes and also picks up any CSS files placed in the application themes directory. Switch between them via the hamburger menu → **Preview Theme**. Changes are applied immediately to all open tabs without a restart.

## Auto-save and draft recovery

- When **Auto-save** is enabled, saved files are written back to disk automatically every 30 seconds.
- Modified tabs are also stored as draft files so unsaved work can be restored on the next launch.
- If a recent file is already open, reopening it switches to the existing tab instead of creating a duplicate.
- Closing a modified tab or window prompts before discarding unsaved work.

## Keyboard shortcuts

### General

| Shortcut | Action |
|---|---|
| Ctrl+N | New tab |
| Ctrl+O | Open file |
| Ctrl+S | Save |
| Ctrl+Shift+S | Save As |
| Ctrl+W | Close current tab |
| Ctrl+PageDown | Next tab |
| Ctrl+PageUp | Previous tab |
| Ctrl+Z | Undo |
| Ctrl+Y | Redo |
| Ctrl+Shift+Z | Redo (alternate) |
| Ctrl+F | Find |
| Ctrl+H | Find and replace |
| Ctrl+1 | View mode: Editor only |
| Ctrl+2 | View mode: Split |
| Ctrl+3 | View mode: Preview only |
| Ctrl+Shift+V | Cycle view mode |
| Ctrl+\ | Toggle split view |
| Ctrl+Shift+D | Toggle dark mode |
| Ctrl+Shift+A | Toggle auto-save |
| Ctrl+Q | Quit |

### Markdown formatting

#### Inline

| Shortcut | Action |
|---|---|
| Ctrl+B | Bold (`**text**`) — toggles on/off |
| Ctrl+I | Italic (`*text*`) — toggles on/off |
| Ctrl+Shift+X | Strikethrough (`~~text~~`) — toggles on/off |
| Ctrl+` | Inline code (`` `text` ``) — toggles on/off |

With text selected the selection is wrapped; with no selection the syntax is inserted with the cursor placed inside.

#### Headings

| Shortcut | Action |
|---|---|
| Ctrl+Alt+1 | Heading 1 (`# `) |
| Ctrl+Alt+2 | Heading 2 (`## `) |
| Ctrl+Alt+3 | Heading 3 (`### `) |
| Ctrl+Alt+4 | Heading 4 (`#### `) |
| Ctrl+Alt+5 | Heading 5 (`##### `) |
| Ctrl+Alt+6 | Heading 6 (`###### `) |

Applies to the entire current line. Pressing the same shortcut again removes the heading. Pressing a different level switches to that level.

#### Links and images

| Shortcut | Action |
|---|---|
| Ctrl+K | Insert link (`[text](url)`) |
| Ctrl+Shift+I | Insert image (`![alt](url)`) |

With text selected, the selection becomes the link text. Tab jumps the cursor from the text field into the URL field.

#### Lists

| Shortcut | Action |
|---|---|
| Ctrl+Shift+L | Bullet list (`- `) — toggles on/off |
| Ctrl+& (Ctrl+Shift+7) | Numbered list (`1. `) — toggles on/off |
| Ctrl+Shift+T | Task list (`- [ ] `) — toggles on/off |

- **Enter** on a non-empty list item continues the list on the next line.
- **Enter** on an empty list item exits the list.
- **Tab** / **Shift+Tab** indents / outdents list items.

#### Blocks

| Shortcut | Action |
|---|---|
| Ctrl+Shift+C | Fenced code block (` ``` `) |
| Ctrl+> (Ctrl+Shift+.) | Block quote (`> `) — toggles on/off |
| Ctrl+_ (Ctrl+Shift+-) | Horizontal rule (`---`) |

Code block wraps the selection if text is selected, otherwise inserts an empty block with the cursor inside.
Block quote and horizontal rule are multi-line aware — the transformation is applied to every selected line.

### Structure and editing

| Shortcut | Action |
|---|---|
| Tab | Indent (4 spaces) or list indent (2 spaces) |
| Shift+Tab | Outdent or list outdent |
| Alt+↑ | Move current line(s) up |
| Alt+↓ | Move current line(s) down |
| Shift+Alt+↓ | Duplicate current line(s) below |
