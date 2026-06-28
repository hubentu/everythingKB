# everythingKB

Personal knowledge base in Rust — OpenKB-style wiki compilation with filesystem discovery.

## Quick start

```bash
# Build
cargo build --release

# Initialize KB
./target/release/everythingkb init

# Pull local LLM (Gemma 4 E2B via Ollama)
ollama pull batiai/gemma4-e2b:q4

# Ingest a folder
./target/release/everythingkb add ~/Documents/notes

# Ask a question (wiki + PageIndex tree navigation)
./target/release/everythingkb query "What topics are in my notes?"

# Interactive chat with SQLite session history
./target/release/everythingkb chat --session mysession

# Scan configured paths (see scan_paths in config.toml)
./target/release/everythingkb scan --dry-run
./target/release/everythingkb scan
```

## Configuration

Default config: `~/.everythingkb/config.toml`  
Default KB data: `~/.everythingkb/kb/`

See [config.example.toml](config.example.toml). LLM inference uses **Ollama** (default model: `batiai/gemma4-e2b:q4`).

```toml
# ~/.everythingkb/config.toml
scan_paths = ["~/Documents", "~", "/media", "/mnt"]

[llm]
ollama_host = "http://127.0.0.1:11434"
ollama_model = "batiai/gemma4-e2b:q4"
n_ctx = 32768
```

Edit `~/.everythingkb/config.toml` to set scan paths and Ollama settings. The legacy key `scan_roots` is still accepted.

Hidden directories (names starting with `.`) are skipped during scan, except when you list a hidden path explicitly in `scan_paths` (e.g. `~/.my-notes`).

### `scan` vs `add`

- **`add <path>`** — ingest one file or folder you point at
- **`scan`** — walk everything in `scan_paths`, ingest only new/changed files (uses the SQLite registry). Use this to keep the KB in sync with your machine.

## Commands

| Command | Description |
|---------|-------------|
| `init` | Create KB, config, registry |
| `scan [--dry-run] [-v]` | Walk `scan_paths`, ingest new/changed files |
| `watch` | Watch scan roots (debounced) |
| `add <path> [--force] [-v]` | Ingest file or directory; `--force` rebuilds unchanged files |
| `query "<q>"` | Query wiki + long-doc trees |
| `chat [--session id]` | REPL with session store |
| `status` | Registry + wiki stats |
| `list` | List indexed files |
| `visualize [--open]` | Interactive wikilink graph → `wiki/graph.html` |

## Pipeline

1. **Scan** — `jwalk` over `scan_paths` with exclusion engine
2. **Convert** — `mdkit` (pdfium, calamine, html) + `undocx` DOCX fallback
3. **Long docs** — pdfium page extract → LLM tree → `pageindex/*.json`
4. **Compile** — OpenKB-style summary → concepts + entities → `index.md`
5. **Query** — wiki context + tree-navigation over PageIndex JSON

By default only **documents** (`pdf`, `docx`, `xlsx`, `csv`, `html`, `md`, `txt`) are indexed. Media is opt-in:

```toml
# Multimodal LLM summary for image files (requires a vision-capable Ollama model)
image = true
llm.vision_model = "llava:7b"  # optional; defaults to llm.ollama_model

# Video/audio metadata stubs (EXIF/ffprobe)
index_media = true

# Software profile paths (.config, saves, mods, userdata)
index_user_profiles = true
```

## Wiki layout

OpenKB-compatible markdown wiki under `wiki/` — opens in Obsidian.

## License

Apache-2.0
