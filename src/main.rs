use std::fs;
use std::path::{Path, PathBuf};
use regex::Regex;
use reqwest;
use url::Url;
use tokio;
use anyhow::{Result, Context};
use walkdir::WalkDir;
use yaml_rust::YamlLoader;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <path>", args[0]);
        eprintln!("Path can be either a file or directory");
        std::process::exit(1);
    }

    let path = &args[1];
    let path = Path::new(path);

    if path.is_file() {
        process_file(path).await?;
    } else if path.is_dir() {
        process_directory(path).await?;
    } else {
        eprintln!("Error: Path does not exist or is not accessible: {}", path.display());
        std::process::exit(1);
    }

    Ok(())
}

async fn process_directory(dir_path: &Path) -> Result<()> {
    // Extended list of file extensions to process
    let text_extensions = vec![
        "txt", "md", "html", "htm", "css", "xml",
        "markdown", "mdown", "mkdn", "mdwn", // Additional markdown extensions
        "hugo", "yml", "yaml", "toml",       // Hugo specific files
    ];

    for entry in WalkDir::new(dir_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok()) 
    {
        let path = entry.path();
        
        if !path.is_file() {
            continue;
        }

        if let Some(ext) = path.extension() {
            if let Some(ext_str) = ext.to_str() {
                if text_extensions.contains(&ext_str.to_lowercase().as_str()) {
                    println!("Processing file: {}", path.display());
                    if let Err(e) = process_file(path).await {
                        eprintln!("Error processing {}: {}", path.display(), e);
                    }
                }
            }
        }
    }
    Ok(())
}

async fn process_file(file_path: &Path) -> Result<()> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;
    
    let base_dir = file_path.parent()
        .unwrap_or_else(|| Path::new(""));

    // Process frontmatter for Hugo markdown files
    let (frontmatter_images, content_without_frontmatter) = extract_frontmatter_images(&content);
    
    // Enhanced patterns to catch more image references
    let patterns = vec![
        // Hugo shortcode patterns
        r#"\{\{<\s*figure\s+src="([^"]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))"[^>]*>\}\}"#,
        r#"\{\{<\s*img\s+src="([^"]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))"[^>]*>\}\}"#,
        
        // Hugo cover image in frontmatter (already handled by frontmatter processing)
        r#"cover:\s*image:\s*"?([^"\n]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))"?\s*$"#,
        
        // Standard Markdown image patterns
        r#"!\[(?:[^\]]*)\]\(([^)]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))\)"#,
        r#"\[(?:[^\]]*)\]:\s*([^)\s]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))"#,
        
        // HTML patterns
        r#"(?:src|href|data)=["']([^"']+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))["']"#,
        r#"style=["'][^"']*background-image:\s*url\(['"]?([^'")]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))['"]?\)"#,
        
        // CSS patterns
        r#"url\(['"]?([^'"\s)]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))['"]?\)"#,
        
        // Catch CDN and other common patterns
        r#"https?://(?:shortcdn\.com|cdn\.com)/[^)\s"']+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp)"#,
    ];

    let mut processed_urls = std::collections::HashSet::new();

    // Process frontmatter images
    for url_str in frontmatter_images {
        process_image_url(&url_str, base_dir, &mut processed_urls).await?;
    }

    // Process content images
    for pattern in patterns {
        let re = Regex::new(pattern)?;
        for cap in re.captures_iter(&content_without_frontmatter) {
            let url_str = &cap[1];
            process_image_url(url_str, base_dir, &mut processed_urls).await?;
        }
    }

    Ok(())
}

fn extract_frontmatter_images(content: &str) -> (Vec<String>, String) {
    let mut images = Vec::new();
    let mut content_without_frontmatter = content.to_string();

    // Check for YAML frontmatter between --- markers
    if content.starts_with("---") {
        if let Some(end_index) = content[3..].find("---") {
            let frontmatter = &content[..end_index + 6];
            content_without_frontmatter = content[end_index + 6..].to_string();

            // Parse YAML frontmatter
            if let Ok(docs) = YamlLoader::load_from_str(frontmatter) {
                if !docs.is_empty() {
                    // Extract cover image
                    if let Some(cover) = docs[0]["cover"]["image"].as_str() {
                        images.push(cover.to_string());
                    }
                    // Extract other image fields that might be present
                    if let Some(banner) = docs[0]["banner"].as_str() {
                        images.push(banner.to_string());
                    }
                    if let Some(thumbnail) = docs[0]["thumbnail"].as_str() {
                        images.push(thumbnail.to_string());
                    }
                }
            }
        }
    }

    (images, content_without_frontmatter)
}

async fn process_image_url(url_str: &str, base_dir: &Path, processed_urls: &mut std::collections::HashSet<String>) -> Result<()> {
    // Skip if already processed
    if processed_urls.contains(url_str) {
        return Ok(());
    }
    processed_urls.insert(url_str.to_string());

    // Try to parse as URL first
    let url = match Url::parse(url_str) {
        Ok(url) => url,
        Err(_) => {
            // Handle relative paths and other formats
            if url_str.starts_with("//") {
                // Protocol-relative URL
                match Url::parse(&format!("https:{}", url_str)) {
                    Ok(url) => url,
                    Err(_) => return Ok(()),
                }
            } else {
                // Skip local file paths
                return Ok(());
            }
        }
    };

    if url.scheme() == "http" || url.scheme() == "https" {
        match download_image(&url, base_dir).await {
            Ok(path) => println!("Downloaded {} to {}", url, path.display()),
            Err(e) => eprintln!("Failed to download {}: {}", url, e),
        }
    }

    Ok(())
}

async fn download_image(url: &Url, base_dir: &Path) -> Result<PathBuf> {
    let filename = url.path_segments()
        .and_then(|segments| segments.last())
        .unwrap_or("downloaded_image")
        .to_string();
    
    // Sanitize filename
    let filename = sanitize_filename(&filename);
    let output_path = base_dir.join(&filename);
    
    // Create parent directories if they don't exist
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    // Skip if file already exists
    if output_path.exists() {
        println!("File already exists: {}", output_path.display());
        return Ok(output_path);
    }
    
    let response = reqwest::get(url.as_str()).await
        .with_context(|| format!("Failed to download from {}", url))?;
    
    // Verify content type is an image
    if let Some(content_type) = response.headers().get("content-type") {
        let content_type = content_type.to_str().unwrap_or("");
        if !content_type.starts_with("image/") {
            return Err(anyhow::anyhow!("Not an image: {} (content-type: {})", url, content_type));
        }
    }
    
    let bytes = response.bytes().await
        .with_context(|| format!("Failed to read response from {}", url))?;
    
    fs::write(&output_path, &bytes)
        .with_context(|| format!("Failed to write file: {}", output_path.display()))?;
    
    Ok(output_path)
}

fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c
        })
        .collect()
}