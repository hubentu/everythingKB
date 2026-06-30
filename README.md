# everythingKB

Personal knowledge base in Rust — [Open Knowledge Format (OKF)](https://cloud.google.com/blog/products/data-analytics/how-the-open-knowledge-format-can-improve-data-sharing) wiki compilation with filesystem discovery.

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

See [config.example.toml](config.example.toml). LLM inference uses **Ollama** by default, or any **OpenAI-compatible** server (vLLM, etc.) via `llm.backend = "openai"`.

```toml
# ~/.everythingkb/config.toml — Ollama (default)
[llm]
ollama_host = "http://127.0.0.1:11434"
ollama_model = "batiai/gemma4-e2b:q4"
n_ctx = 32768
```

```toml
# vLLM on LAN (OpenAI-compatible API)
[llm]
backend = "openai"
openai_base_url = "http://192.168.1.167:8000/v1"
openai_model = "/model"   # from: curl http://192.168.1.167:8000/v1/models
n_ctx = 8192
temperature = 0.3
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
| `visualize [--open]` | Interactive knowledge graph → `wiki/graph.html` |

## Pipeline

1. **Scan** — `jwalk` over `scan_paths` with exclusion engine
2. **Convert** — `mdkit` (calamine, html, csv) + bundled pdfium PDF + `undocx` DOCX fallback
3. **Long docs** — pdfium page extract → LLM tree → `pageindex/*.json`

PDF support auto-downloads `libpdfium` (chromium/7920) to `~/.cache/everythingkb/pdfium-7920/` on first PDF ingest. No `LD_LIBRARY_PATH` or manual install.
4. **Compile** — OKF summaries (`type`, `resource`, `timestamp`) → concepts + entities → `index.md`
5. **Query** — wiki context + tree-navigation over PageIndex JSON

Wiki output follows [OKF v0.1](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md): markdown + YAML frontmatter, standard `[label](path.md)` cross-links, bundle `index.md` + `log.md`.

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

### Private / sensitive documents

Documents with personal or sensitive content go in a **separate private wiki** under `wiki/private/` (summaries, concepts, entities, sources). Public chat/query never sees them.

Two ways to mark private:

1. **Path rule** — list folders in `private_paths` (always private):
```toml
private_paths = ["~/Documents/medical", "~/Documents/tax"]
```

2. **LLM detection** — during compile, the model sets `"private": true` for PII, medical, financial, or similar content (`private_detect = true`, default).

Use the private wiki for chat/query:

```bash
everythingkb chat --private --session personal
everythingkb query --private "What was my diagnosis?"
```

Public commands (`chat`, `query`, `visualize`) use only the public wiki by default. Add `--private` to include or target the private zone.

## Wiki layout

OpenKB-compatible markdown wiki under `wiki/` — opens in Obsidian.

## License

Apache-2.0
