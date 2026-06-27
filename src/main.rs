use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use anyhow::{Result, Context};
use clap::Parser;
use reqwest::Client;

const REQUEST_TIMEOUT_SECS: u64 = 30;
use regex::Regex;
use serde_json::Value;
use serde_yml;
use toml::Value as TomlValue;
use url::Url;

#[derive(Parser)]
#[command(version, about = "Download images referenced in text-based files")]
struct Args {
    /// Directory or file to process
    #[arg(default_value = ".")]
    path: PathBuf,
}

struct Config {
    client: Client,
}

// Struct to hold parsed front matter information
#[derive(Debug)]
struct FrontMatter {
    content: String,
    format: FrontMatterFormat,
}

#[derive(Debug)]
enum FrontMatterFormat {
    YAML,
    TOML,
    JSON,
    None,
}

impl FrontMatter {
    fn parse(content: &str) -> Result<Self> {
        // Check for YAML front matter (---)
        if content.starts_with("---") {
            if let Some(end) = content[3..].find("---") {
                return Ok(FrontMatter {
                    content: content[3..end + 3].to_string(),
                    format: FrontMatterFormat::YAML,
                });
            }
        }
        
        // Check for TOML front matter (+++)
        if content.starts_with("+++") {
            if let Some(end) = content[3..].find("+++") {
                return Ok(FrontMatter {
                    content: content[3..end + 3].to_string(),
                    format: FrontMatterFormat::TOML,
                });
            }
        }
        
        // Check for JSON front matter ({)
        if content.starts_with("{") {
            if let Some(end) = find_json_end(content) {
                return Ok(FrontMatter {
                    content: content[..end + 1].to_string(),
                    format: FrontMatterFormat::JSON,
                });
            }
        }

        // No front matter found
        Ok(FrontMatter {
            content: String::new(),
            format: FrontMatterFormat::None,
        })
    }
}

fn find_json_end(content: &str) -> Option<usize> {
    let mut brace_count = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, c) in content.chars().enumerate() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' if !in_string => brace_count += 1,
            '}' if !in_string => {
                brace_count -= 1;
                if brace_count == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

async fn process_file(file_path: &Path, config: &Config) -> Result<()> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;
    
    let base_dir = file_path.parent()
        .unwrap_or_else(|| Path::new(""));

    let mut processed_urls = std::collections::HashSet::new();

    // First try to parse as a complete JSON file
    if content.trim().starts_with("{") {
        if let Ok(json) = serde_json::from_str::<Value>(&content) {
            let mut urls = Vec::new();
            collect_urls_from_value(&json, &mut urls);
            for url_str in urls {
                process_url(&url_str, &mut processed_urls, base_dir, config).await?;
            }
        }
    }

    // Extract and process front matter
    let front_matter = FrontMatter::parse(&content)?;
    match front_matter.format {
        FrontMatterFormat::YAML => {
            if let Ok(yaml) = serde_yml::from_str::<Value>(&front_matter.content) {
                let mut urls = Vec::new();
                collect_urls_from_value(&yaml, &mut urls);
                for url_str in urls {
                    process_url(&url_str, &mut processed_urls, base_dir, config).await?;
                }
            }
        },
        FrontMatterFormat::TOML => {
            if let Ok(toml) = front_matter.content.parse::<TomlValue>() {
                let mut urls = Vec::new();
                collect_urls_from_toml(&toml, &mut urls);
                for url_str in urls {
                    process_url(&url_str, &mut processed_urls, base_dir, config).await?;
                }
            }
        },
        FrontMatterFormat::JSON => {
            if let Ok(json) = serde_json::from_str::<Value>(&front_matter.content) {
                let mut urls = Vec::new();
                collect_urls_from_value(&json, &mut urls);
                for url_str in urls {
                    process_url(&url_str, &mut processed_urls, base_dir, config).await?;
                }
            }
        },
        FrontMatterFormat::None => {}
    }

    process_content_with_patterns(&content, &mut processed_urls, base_dir, config).await?;

    Ok(())
}

fn collect_urls_from_value(value: &Value, urls: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            if looks_like_image_url(s) {
                urls.push(s.clone());
            }
        },
        Value::Object(map) => {
            for (_, v) in map {
                collect_urls_from_value(v, urls);
            }
        },
        Value::Array(arr) => {
            for v in arr {
                collect_urls_from_value(v, urls);
            }
        },
        _ => {}
    }
}

fn collect_urls_from_toml(value: &TomlValue, urls: &mut Vec<String>) {
    match value {
        TomlValue::String(s) => {
            if looks_like_image_url(s) {
                urls.push(s.clone());
            }
        },
        TomlValue::Table(table) => {
            for (_, v) in table {
                collect_urls_from_toml(v, urls);
            }
        },
        TomlValue::Array(arr) => {
            for v in arr {
                collect_urls_from_toml(v, urls);
            }
        },
        _ => {}
    }
}

static CONTENT_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();

fn content_patterns() -> &'static Vec<Regex> {
    CONTENT_PATTERNS.get_or_init(|| {
        [
            r#"(?:src|href)=["']?([^"'\s>]+\.(?:jpg|jpeg|png|svg|webp|gif))["']?"#,
            r#"!\[.*?\]\(([^)]+\.(?:jpg|jpeg|png|svg|webp|gif))\)"#,
            r#"(?:url\(['"]?)([^'")\s]+\.(?:jpg|jpeg|png|svg|webp|gif))['"]?\)"#,
            r#"(?m)^(?:image|cover|featured_image|thumbnail|banner|avatar|logo):\s*["']?([^"'\s\[]+\.(?:jpg|jpeg|png|svg|webp|gif))["']?\s*$"#,
            r#"(?m)^\s+(?:image|caption|icon):\s*["']?([^"'\s\[]+\.(?:jpg|jpeg|png|svg|webp|gif))["']?\s*$"#,
            r#"["']?(?:image|cover|featured_image|thumbnail)["']?\s*[:=]\s*["']([^"']+\.(?:jpg|jpeg|png|svg|webp|gif))["']"#,
        ]
        .iter()
        .map(|p| Regex::new(p).expect("invalid pattern"))
        .collect()
    })
}

fn looks_like_image_url(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.ends_with(".jpg") || lower.ends_with(".jpeg") || 
    lower.ends_with(".png") || lower.ends_with(".svg") || 
    lower.ends_with(".webp") || lower.ends_with(".gif")
}

async fn process_url(url_str: &str, processed_urls: &mut std::collections::HashSet<String>, base_dir: &Path, config: &Config) -> Result<()> {
    if processed_urls.contains(url_str) {
        return Ok(());
    }
    processed_urls.insert(url_str.to_string());

    let url = if let Ok(parsed_url) = Url::parse(url_str) {
        parsed_url
    } else {
        let path = if url_str.starts_with('/') {
            // TODO: Handle site root configuration
            return Ok(());
        } else {
            base_dir.join(url_str)
        };
        
        match Url::from_file_path(path) {
            Ok(u) => u,
            Err(_) => return Ok(()),
        }
    };
    
    if url.scheme() == "http" || url.scheme() == "https" {
        match download_image(&url, base_dir, config).await {
            Ok(path) => println!("Downloaded {} to {}", url, path.display()),
            Err(e) => eprintln!("Failed to download {}: {}", url, e),
        }
    }

    Ok(())
}

async fn process_content_with_patterns(
    content: &str,
    processed_urls: &mut std::collections::HashSet<String>,
    base_dir: &Path,
    config: &Config,
) -> Result<()> {
    for re in content_patterns() {
        for cap in re.captures_iter(content) {
            let url_str = &cap[1];
            process_url(url_str, processed_urls, base_dir, config).await?;
        }
    }
    Ok(())
}

async fn download_image(url: &Url, base_dir: &Path, config: &Config) -> Result<PathBuf> {
    let response = config.client
        .get(url.as_str())
        .send()
        .await
        .with_context(|| format!("Failed to fetch {}", url))?;

    let filename = url
        .path_segments()
        .and_then(|mut segs| segs.next_back())
        .filter(|s| !s.is_empty())
        .unwrap_or("image");

    let dest_path = base_dir.join(filename);

    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("Failed to read response body for {}", url))?;

    fs::write(&dest_path, &bytes)
        .with_context(|| format!("Failed to write to {}", dest_path.display()))?;

    Ok(dest_path)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let client = Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .context("Failed to build HTTP client")?;
    let config = Config { client };

    for entry in walkdir::WalkDir::new(&args.path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext.to_lowercase().as_str() {
                "md" | "html" | "yaml" | "yml" | "toml" | "json" => {
                    if let Err(e) = process_file(path, &config).await {
                        eprintln!("Error processing {}: {}", path.display(), e);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}