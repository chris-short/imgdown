# imgdown

A concurrent Rust CLI utility that scans text files (HTML, MD, CSS) to download referenced images, featuring parallel processing, multiple format support, and configurable download settings.

## Features

- **Concurrent Processing**: Handles multiple files and downloads simultaneously
- **Format Support**: Detects images in:
  - Hugo shortcodes and frontmatter
  - Markdown image syntax
  - HTML img tags and backgrounds
  - CSS background images
  - Various URL formats
- **Smart Downloading**: 
  - Maintains directory structure
  - Skips existing files
  - Verifies content types
  - Handles various image formats (JPG, PNG, SVG, WebP, GIF, AVIF, ICO, BMP)
- **Configurable**: All major settings can be adjusted via command line flags
- **Progress Tracking**: Detailed progress reporting for all operations

## Installation

```bash
# Clone the repository
git clone https://github.com/username/imgdown
cd imgdown

# Build with Cargo
cargo build --release

# Optional: Install system-wide
cargo install --path .
```

## Usage

Basic usage with default settings:
```bash
imgdown path/to/files
```

### Command Line Options

```
OPTIONS:
    -f, --max-concurrent-files <N>     Maximum files to process [default: 50]
    -d, --max-concurrent-downloads <N>  Maximum downloads per file [default: 25]
    -t, --timeout-seconds <N>          Download timeout [default: 30]
    -e, --extensions <LIST>            File extensions to process [default: txt,md,html,htm,css,xml,...]
    -v, --verbose                      Enable detailed output
    -h, --help                         Show help information
```

### Examples

Process a single file:
```bash
imgdown article.md
```

Process a directory with custom settings:
```bash
imgdown ./blog --max-concurrent-files 100 --max-concurrent-downloads 50
```

Process specific file types with longer timeout:
```bash
imgdown ./docs --extensions "md,html" --timeout-seconds 60
```

Enable verbose output:
```bash
imgdown ./content --verbose
```

## Supported Image References

The utility can detect and download images referenced in various formats:

### Markdown
```markdown
![Alt text](https://example.com/image.jpg)
[Ref]: https://example.com/image.png
```

### Hugo Shortcodes
```markdown
{{< figure src="image.jpg" >}}
{{< img src="image.png" >}}
```

### HTML
```html
<img src="image.jpg">
<div style="background-image: url('image.png')">
```

### CSS
```css
.background {
    background-image: url('image.jpg');
}
```

### Hugo Frontmatter
```yaml
---
cover:
  image: featured-image.jpg
banner: header.png
---
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.