use std::fs;
use std::path::{Path, PathBuf};
use regex::Regex;
use reqwest;
use url::Url;
use tokio;
use futures::stream::{self, StreamExt};
use anyhow::{Result, Context};
use walkdir::WalkDir;
use yaml_rust::YamlLoader;
use std::sync::Arc;
use tokio::sync::Semaphore;
use clap::{Parser, ArgAction};

#[derive(Parser, Debug, Clone)]
#[command(
    version,
    about = "A concurrent image downloader for markdown files",
    long_about = "Downloads images from markdown and other text files concurrently. Only the path argument is required - all other arguments have sensible defaults.",
)]
struct Args {
    /// Path to file or directory to process
    #[arg(required = true, help = "Path to file or directory to process - this is the only required argument")]
    path: String,

    /// Maximum number of files to process concurrently (default: 50)
    #[arg(
        short = 'f',
        long,
        default_value = "50",
        help = "Maximum number of files to process concurrently (default: 50)"
    )]
    max_concurrent_files: usize,

    /// Maximum number of concurrent downloads per file (default: 25)
    #[arg(
        short = 'd',
        long,
        default_value = "25",
        help = "Maximum number of concurrent downloads per file (default: 25)"
    )]
    max_concurrent_downloads: usize,

    /// Request timeout in seconds (default: 30)
    #[arg(
        short = 't',
        long,
        default_value = "30",
        help = "Request timeout in seconds (default: 30)"
    )]
    timeout_seconds: u64,

    /// List of file extensions to process, comma-separated
    #[arg(
        short = 'e',
        long,
        default_value = "txt,md,html,htm,css,xml,markdown,mdown,mkdn,mdwn,hugo,yml,yaml,toml",
        help = "List of file extensions to process, comma-separated (default: txt,md,html,htm,css,xml,markdown,mdown,mkdn,mdwn,hugo,yml,yaml,toml)"
    )]
    extensions: String,

    /// Print verbose output
    #[arg(
        short,
        long,
        action = ArgAction::SetTrue,
        help = "Enable verbose output (default: false)",
        default_value = "false"
    )]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let path = Path::new(&args.path);

    if path.is_file() {
        process_file(path, &args).await?;
    } else if path.is_dir() {
        process_directory(path, &args).await?;
    } else {
        eprintln!("Error: Path does not exist or is not accessible: {}", path.display());
        std::process::exit(1);
    }

    Ok(())
}

async fn process_directory(dir_path: &Path, args: &Args) -> Result<()> {
    println!("Scanning directory: {}", dir_path.display());
    let text_extensions: Vec<&str> = args.extensions.split(',').collect();

    // Collect all files first
    let files: Vec<PathBuf> = WalkDir::new(dir_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| text_extensions.contains(&ext.to_lowercase().as_str()))
                .unwrap_or(false)
        })
        .map(|e| e.path().to_owned())
        .collect();

    let file_count = files.len();
    println!("Found {} files to process", file_count);

    let semaphore = Arc::new(Semaphore::new(args.max_concurrent_files));
    let processed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    
    stream::iter(files)
        .map(|path| {
            let sem = Arc::clone(&semaphore);
            let args = args.to_owned();
            let path = path.clone();
            let processed = Arc::clone(&processed);
            async move {
                let _permit = sem.acquire().await.unwrap();
                if let Err(e) = process_file(&path, &args).await {
                    eprintln!("Error processing {}: {}", path.display(), e);
                }
                let current = processed.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                println!("Progress: {}/{} files processed", current, file_count);
            }
        })
        .buffer_unordered(args.max_concurrent_files)
        .collect::<Vec<()>>()
        .await;

    println!("Directory processing complete: {}", dir_path.display());
    Ok(())
}

async fn process_file(file_path: &Path, args: &Args) -> Result<()> {
    if args.verbose {
        println!("Processing file: {}", file_path.display());
    }
    
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;
    
    let base_dir = file_path.parent()
        .unwrap_or_else(|| Path::new(""));

    let (frontmatter_images, content_without_frontmatter) = extract_frontmatter_images(&content);
    
    let patterns = vec![
        r#"\{\{<\s*figure\s+src="([^"]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))"[^>]*>\}\}"#,
        r#"\{\{<\s*img\s+src="([^"]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))"[^>]*>\}\}"#,
        r#"cover:\s*image:\s*"?([^"\n]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))"?\s*$"#,
        r#"!\[(?:[^\]]*)\]\(([^)]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))\)"#,
        r#"\[(?:[^\]]*)\]:\s*([^)\s]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))"#,
        r#"(?:src|href|data)=["']([^"']+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))["']"#,
        r#"style=["'][^"']*background-image:\s*url\(['"]?([^'")]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))['"]?\)"#,
        r#"url\(['"]?([^'"\s)]+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp))['"]?\)"#,
        r#"https?://(?:shortcdn\.com|cdn\.com)/[^)\s"']+\.(?:jpg|jpeg|png|gif|svg|webp|avif|ico|bmp)"#,
    ];

    let mut urls = Vec::new();

    urls.extend(frontmatter_images);

    for pattern in patterns {
        let re = Regex::new(pattern)?;
        for cap in re.captures_iter(&content_without_frontmatter) {
            if let Some(url_match) = cap.get(1) {
                urls.push(url_match.as_str().to_string());
            }
        }
    }

    if args.verbose && !urls.is_empty() {
        println!("Found {} images in {}", urls.len(), file_path.display());
    }

    let urls: Vec<String> = urls.into_iter().collect();
    let base_dir = base_dir.to_path_buf();
    
    let semaphore = Arc::new(Semaphore::new(args.max_concurrent_downloads));
    
    stream::iter(urls)
        .map(|url| {
            let sem = Arc::clone(&semaphore);
            let base_dir = base_dir.clone();
            let args = args.clone();
            let url = url.to_string();
            async move {
                let _permit = sem.acquire().await.unwrap();
                if let Err(e) = process_image_url(&url, &base_dir, args.timeout_seconds, args.verbose).await {
                    eprintln!("Error downloading {}: {}", url, e);
                }
            }
        })
        .buffer_unordered(args.max_concurrent_downloads)
        .collect::<Vec<()>>()
        .await;

    Ok(())
}

async fn process_image_url(url_str: &str, base_dir: &Path, timeout_seconds: u64, verbose: bool) -> Result<()> {
    let url = match Url::parse(url_str) {
        Ok(url) => url,
        Err(_) => {
            if url_str.starts_with("//") {
                match Url::parse(&format!("https:{}", url_str)) {
                    Ok(url) => url,
                    Err(_) => return Ok(()),
                }
            } else {
                return Ok(());
            }
        }
    };

    if url.scheme() == "http" || url.scheme() == "https" {
        match download_image(&url, base_dir, timeout_seconds, verbose).await {
            Ok(path) => {
                if verbose {
                    println!("Downloaded {} to {}", url, path.display());
                }
            },
            Err(e) => eprintln!("Failed to download {}: {}", url, e),
        }
    }

    Ok(())
}

async fn download_image(url: &Url, base_dir: &Path, timeout_seconds: u64, verbose: bool) -> Result<PathBuf> {
    let filename = url.path_segments()
        .and_then(|segments| segments.last())
        .unwrap_or("downloaded_image")
        .to_string();
    
    let filename = sanitize_filename(&filename);
    let output_path = base_dir.join(&filename);
    
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    if output_path.exists() {
        if verbose {
            println!("File already exists: {}", output_path.display());
        }
        return Ok(output_path);
    }
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_seconds))
        .build()?;
        
    let response = client.get(url.as_str()).send().await
        .with_context(|| format!("Failed to download from {}", url))?;
    
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

fn extract_frontmatter_images(content: &str) -> (Vec<String>, String) {
    let mut images = Vec::new();
    let mut content_without_frontmatter = content.to_string();

    if content.starts_with("---") {
        if let Some(end_index) = content[3..].find("---") {
            let frontmatter = &content[..end_index + 6];
            content_without_frontmatter = content[end_index + 6..].to_string();

            if let Ok(docs) = YamlLoader::load_from_str(frontmatter) {
                if !docs.is_empty() {
                    if let Some(cover) = docs[0]["cover"]["image"].as_str() {
                        images.push(cover.to_string());
                    }
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

fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c
        })
        .collect()
}