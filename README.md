# imgdown

Built to pull images off a [CDN](https://bunny.net?ref=ntj8lzdwyl) into [Hugo Page Bundles](https://gohugo.io/content-management/page-bundles/).

A Rust utility that finds image references in text-based files and downloads them into the same directory as the source file.

## Features

- Processes individual files or entire directories recursively
- Supports multiple file formats: Markdown, HTML, YAML, TOML, JSON
- Handles image formats: JPG, JPEG, PNG, SVG, WebP, GIF
- Parses front matter (YAML `---`, TOML `+++`, JSON `{`)
- Skips images that already exist locally
- Concurrent downloads per file
- Dry-run mode to preview without downloading
- Root-relative URL resolution via `--base-url`

## Prerequisites

- Rust (latest stable)
- Cargo

## Installation

```bash
git clone https://github.com/chris-short/imgdown
cd imgdown
cargo build --release
```

The binary will be at `target/release/imgdown`.

## Usage

```bash
# Process current directory
imgdown

# Process a specific directory or file
imgdown path/to/directory
imgdown path/to/file.md

# Preview what would be downloaded without touching anything
imgdown --dry-run ./content

# Resolve root-relative paths like /images/foo.jpg
imgdown --base-url https://cdn.example.com ./content

# Allow plain HTTP URLs (not recommended)
imgdown --allow-http ./content
```

### Options

| Flag | Description |
|------|-------------|
| `--dry-run` | Print what would be downloaded; no network or disk activity |
| `--base-url <URL>` | Base URL for resolving root-relative paths (e.g. `/images/foo.jpg`) |
| `--allow-http` | Allow insecure HTTP downloads (HTTPS only by default) |
| `-h, --help` | Print help |
| `-V, --version` | Print version |

## How It Works

1. Walks the target path for `.md`, `.html`, `.yaml`, `.yml`, `.toml`, and `.json` files
2. For each file, extracts image URLs from:
   - Front matter (YAML, TOML, or JSON blocks at the top of the file)
   - Inline content via regex (Markdown image syntax, HTML `src`/`href`, CSS `url()`, and common front matter key names)
3. Deduplicates URLs, then downloads all of them concurrently into the same directory as the source file
4. Skips any URL that already exists on disk

## Security

- HTTPS by default; plain HTTP requires `--allow-http`
- 30-second request timeout
- 50 MB per-image download limit
- Content-Type must start with `image/` or the download is rejected
- HTTP 4xx/5xx responses are treated as errors, not written to disk
- Uses [rustls](https://github.com/rustls/rustls) for TLS

## Limitations

- Does not process JavaScript-generated image references
- Root-relative URLs (e.g. `/images/foo.jpg`) require `--base-url`
- Does not rewrite source files to update paths after downloading

## License

[MIT License](LICENSE)

## Authors

Chris Short <chrisshort@duck.com>
