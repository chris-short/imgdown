use std::fs;
use std::path::{Path, PathBuf};
use regex::Regex;
use reqwest;
use url::Url;
use tokio;
use anyhow::{Result, Context};
use walkdir::WalkDir;

#[tokio::main]
async  fn main() -> Result<()> {
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
    // Define file extensions we want to process
    let text_extensions = vec!["txt", "md", "html", "htm", "css", "xml"];

    for entry in WalkDir::new(dir_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok()) 
    {
        let path = entry.path();
        
        // Skip if not a file
        if !path.is_file() {
            continue;
        }

        // Check if file extension is one we want to process
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

    // Match common image patterns in HTML, Markdown, and other formats
    let patterns = vec![
        r#"(?:src|href)=["']([^"']+\.(?:jpg|jpeg|png|svg|webp))["']"#,  // HTML
        r#"!\[.*?\]\(([^)]+\.(?:jpg|jpeg|png|svg|webp))\)"#,            // Markdown
        r#"(?:url\(['"]?)([^'")\s]+\.(?:jpg|jpeg|png|svg|webp))['"]?\)"#, // CSS
    ];

    let mut processed_urls = std::collections::HashSet::new();

    for pattern in patterns {
        let re = Regex::new(pattern)?;
        for cap in re.captures_iter(&content) {
            let url_str = &cap[1];
            
            if let Ok(url) = Url::parse(url_str) {
                // Skip if we've already processed this URL
                if processed_urls.contains(url_str) {
                    continue;
                }
                processed_urls.insert(url_str.to_string());

                if url.scheme() == "http" || url.scheme() == "https" {
                    match download_image(&url, base_dir).await {
                        Ok(path) => println!("Downloaded {} to {}", url, path.display()),
                        Err(e) => eprintln!("Failed to download {}: {}", url, e),
                    }
                }
            }
        }
    }

    Ok(())
}

async fn download_image(url: &Url, base_dir: &Path) -> Result<PathBuf> {
    let filename = url.path_segments()
        .and_then(|segments| segments.last())
        .unwrap_or("downloaded_image")
        .to_string();
    
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
    
    let bytes = response.bytes().await
        .with_context(|| format!("Failed to read response from {}", url))?;
    
    fs::write(&output_path, &bytes)
        .with_context(|| format!("Failed to write file: {}", output_path.display()))?;
    
    Ok(output_path)
}
