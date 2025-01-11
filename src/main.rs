use std::fs;
use std::path::{Path, PathBuf};
use regex::Regex;
use reqwest;
use url::Url;
use serde_json::Value;
use serde_yaml;
use toml::Value as TomlValue;
use anyhow::{Result, Context, anyhow};

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

async fn process_file(file_path: &Path) -> Result<()> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;
    
    let base_dir = file_path.parent()
        .unwrap_or_else(|| Path::new(""));

    let mut processed_urls = std::collections::HashSet::new();

    // First try to parse as a complete JSON file
    if content.trim().starts_with("{") {
        if let Ok(json) = serde_json::from_str::<Value>(&content) {
            process_json_value(&json, &mut processed_urls, base_dir).await?;
        }
    }

    // Extract and process front matter
    let front_matter = FrontMatter::parse(&content)?;
    match front_matter.format {
        FrontMatterFormat::YAML => {
            if let Ok(yaml) = serde_yaml::from_str::<Value>(&front_matter.content) {
                process_yaml_value(&yaml, &mut processed_urls, base_dir).await?;
            }
        },
        FrontMatterFormat::TOML => {
            if let Ok(toml) = front_matter.content.parse::<TomlValue>() {
                process_toml_value(&toml, &mut processed_urls, base_dir).await?;
            }
        },
        FrontMatterFormat::JSON => {
            if let Ok(json) = serde_json::from_str::<Value>(&front_matter.content) {
                process_json_value(&json, &mut processed_urls, base_dir).await?;
            }
        },
        FrontMatterFormat::None => {}
    }

    // Process regular content with regex patterns
    let patterns = vec![
        // HTML/Markdown patterns
        r#"(?:src|href)=["']([^"']+\.(?:jpg|jpeg|png|svg|webp|gif))["']"#,
        r#"!\[.*?\]\(([^)]+\.(?:jpg|jpeg|png|svg|webp|gif))\)"#,
        r#"(?:url\(['"]?)([^'")\s]+\.(?:jpg|jpeg|png|svg|webp|gif))['"]?\)"#,
        
        // Additional patterns for various formats
        r#"(?m)^(?:image|cover|featured_image|thumbnail|banner|avatar|logo):\s*["']?([^"'\s\[]+\.(?:jpg|jpeg|png|svg|webp|gif))["']?\s*$"#,
        r#"(?m)^\s+(?:image|caption|icon):\s*["']?([^"'\s\[]+\.(?:jpg|jpeg|png|svg|webp|gif))["']?\s*$"#,
        r#"["']?(?:image|cover|featured_image|thumbnail)["']?\s*[:=]\s*["']([^"']+\.(?:jpg|jpeg|png|svg|webp|gif))["']"#,
    ];

    process_content_with_patterns(&content, &patterns, &mut processed_urls, base_dir).await?;

    Ok(())
}

async fn process_yaml_value(value: &Value, processed_urls: &mut std::collections::HashSet<String>, base_dir: &Path) -> Result<()> {
    match value {
        Value::String(s) => {
            if looks_like_image_url(s) {
                process_url(s, processed_urls, base_dir).await?;
            }
        },
        Value::Mapping(map) => {
            for (_, value) in map {
                process_yaml_value(value, processed_urls, base_dir).await?;
            }
        },
        Value::Sequence(seq) => {
            for value in seq {
                process_yaml_value(value, processed_urls, base_dir).await?;
            }
        },
        _ => {}
    }
    Ok(())
}

async fn process_json_value(value: &Value, processed_urls: &mut std::collections::HashSet<String>, base_dir: &Path) -> Result<()> {
    match value {
        Value::String(s) => {
            if looks_like_image_url(s) {
                process_url(s, processed_urls, base_dir).await?;
            }
        },
        Value::Object(obj) => {
            for (_, value) in obj {
                process_json_value(value, processed_urls, base_dir).await?;
            }
        },
        Value::Array(arr) => {
            for value in arr {
                process_json_value(value, processed_urls, base_dir).await?;
            }
        },
        _ => {}
    }
    Ok(())
}

async fn process_toml_value(value: &TomlValue, processed_urls: &mut std::collections::HashSet<String>, base_dir: &Path) -> Result<()> {
    match value {
        TomlValue::String(s) => {
            if looks_like_image_url(s) {
                process_url(s, processed_urls, base_dir).await?;
            }
        },
        TomlValue::Table(table) => {
            for (_, value) in table {
                process_toml_value(value, processed_urls, base_dir).await?;
            }
        },
        TomlValue::Array(arr) => {
            for value in arr {
                process_toml_value(value, processed_urls, base_dir).await?;
            }
        },
        _ => {}
    }
    Ok(())
}

fn looks_like_image_url(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.ends_with(".jpg") || lower.ends_with(".jpeg") || 
    lower.ends_with(".png") || lower.ends_with(".svg") || 
    lower.ends_with(".webp") || lower.ends_with(".gif")
}

async fn process_url(url_str: &str, processed_urls: &mut std::collections::HashSet<String>, base_dir: &Path) -> Result<()> {
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
        match download_image(&url, base_dir).await {
            Ok(path) => println!("Downloaded {} to {}", url, path.display()),
            Err(e) => eprintln!("Failed to download {}: {}", url, e),
        }
    }

    Ok(())
}

async fn process_content_with_patterns(
    content: &str, 
    patterns: &[&str], 
    processed_urls: &mut std::collections::HashSet<String>,
    base_dir: &Path
) -> Result<()> {
    for pattern in patterns {
        let re = Regex::new(pattern)?;
        for cap in re.captures_iter(content) {
            let url_str = &cap[1];
            process_url(url_str, processed_urls, base_dir).await?;
        }
    }
    Ok(())
}