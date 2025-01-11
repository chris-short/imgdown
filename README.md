# imgdown

A note from Chris: I built this tool to help me yank files off my [CDN](https://bunny.net?ref=ntj8lzdwyl) into [Hugo Page Bundles](https://gohugo.io/content-management/page-bundles/)

A Rust utility that automatically downloads images referenced in text-based files like HTML, Markdown, and CSS documents.

## Features

- Processes individual files or entire directories recursively
- Supports multiple file formats:
  - HTML (.html, .htm)
  - Markdown (.md)
  - CSS (.css)
  - Plain text (.txt)
  - XML (.xml)
- Handles various image formats: JPG, JPEG, PNG, SVG, and WebP
- Supports both relative and absolute URLs
- Maintains original file structure
- Skips existing files to avoid duplicate downloads
- Follows symbolic links when scanning directories

## Prerequisites

- Rust (latest stable version)
- Cargo package manager

## Dependencies

```toml
[dependencies]
tokio = { version = "1.0", features = ["full"] }
reqwest = "0.11"
anyhow = "1.0"
regex = "1.0"
url = "2.0"
walkdir = "2.0"
```

## Installation

1. Clone the repository:
```bash
git clone [repository-url]
cd imgdown
```

2. Build the project:
```bash
cargo build --release
```

The compiled binary will be available in `target/release/`.

## Usage

The application can process either a single file or an entire directory:

```bash
# Process a single file
./imgdown path/to/file.html

# Process an entire directory
./imgdown path/to/directory
```

### Example

```bash
./imgdown ./docs/blog
```

This will:

1. Scan all supported files in the `./docs/blog` directory
2. Find image references in these files
3. Download the images to the same directory structure as their referencing files
4. Skip any images that have already been downloaded

## How It Works

1. The program accepts a file or directory path as input
2. For directories, it recursively scans for supported file types
3. For each file, it:
   - Reads the content
   - Uses regular expressions to find image references
   - Downloads images from valid URLs
   - Preserves the directory structure
   - Skips existing files

## Error Handling

- Invalid paths result in appropriate error messages
- Download failures are logged but don't stop the process
- File access issues are reported with detailed error messages

## Limitations

- Only processes files with supported extensions
- Requires valid URL formatting in source files
- Does not validate image content
- Does not process JavaScript-generated image references

## Contributing

Contributions are welcome! Here are some ways you can contribute:

- Report bugs
- Suggest new features
- Add support for more file types
- Improve error handling
- Enhance documentation

## License

[MIT License](LICENSE)

## Authors

Chris Short <chrisshort@duck.com>