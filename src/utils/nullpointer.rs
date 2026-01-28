use reqwest::multipart;
use std::path::Path;
use rand::Rng;
use std::time::Duration;

const UPLOAD_URLS: &[&str] = &[
    "https://litterbox.catbox.moe/resources/internals/api.php",
    "https://catbox.moe/user/api.php",
    "https://x0.at",
    "https://0x0.st",
];

#[derive(Debug)]
pub struct UploadResult {
    pub url: String,
    pub token: Option<String>,
    pub host: String,
}

pub async fn upload_file(file_path: &Path) -> Result<UploadResult, anyhow::Error> {
    let filename = file_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    
    let content = tokio::fs::read(file_path).await?;
    upload_bytes(&content, &filename).await
}

pub async fn upload_bytes(data: &[u8], filename: &str) -> Result<UploadResult, anyhow::Error> {
    let mut last_error = anyhow::anyhow!("No upload hosts available");
    
    let mut hosts: Vec<&str> = UPLOAD_URLS.to_vec();
    let mut rng = rand::thread_rng();
    for i in (1..hosts.len()).rev() {
        let j = rng.gen_range(0..=i);
        hosts.swap(i, j);
    }
    
    for host in hosts {
        match try_upload(host, data, filename).await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = e;
                continue;
            }
        }
    }
    
    Err(last_error)
}

async fn try_upload(host: &str, data: &[u8], filename: &str) -> Result<UploadResult, anyhow::Error> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(30))
        .build()?;
    
    let part = multipart::Part::bytes(data.to_vec())
        .file_name(filename.to_string())
        .mime_str("application/octet-stream")?;
    
    let form = if host.contains("catbox.moe") {
        if host.contains("litterbox") {
            multipart::Form::new()
                .text("reqtype", "fileupload")
                .text("time", "72h")
                .part("fileToUpload", part)
        } else {
            multipart::Form::new()
                .text("reqtype", "fileupload")
                .text("userhash", "")
                .part("fileToUpload", part)
        }
    } else {
        multipart::Form::new().part("file", part)
    };
    
    let response = client
        .post(host)
        .multipart(form)
        .send()
        .await?;
    
    let token = response
        .headers()
        .get("x-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Upload failed ({}): {}", status, body));
    }
    
    let url = response.text().await?.trim().to_string();
    
    Ok(UploadResult { 
        url, 
        token,
        host: host.to_string(),
    })
}

pub async fn upload_with_zip(file_path: &Path) -> Result<UploadResult, anyhow::Error> {
    use std::io::{Cursor, Write};
    use zip::ZipWriter;
    use zip::write::FileOptions;
    
    let filename = file_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    
    let content = tokio::fs::read(file_path).await?;
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buffer);
        let options = FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        
        zip.start_file(&filename, options)?;
        zip.write_all(&content)?;
        zip.finish()?;
    }
    
    let zip_data = buffer.into_inner();
    let zip_filename = format!("{}.zip", filename);
    
    upload_bytes(&zip_data, &zip_filename).await
}

pub async fn delete_file(url: &str, token: &str) -> Result<(), anyhow::Error> {
    let client = reqwest::Client::builder()
        .user_agent("curl/8.0.0")
        .build()?;
    
    let form = multipart::Form::new()
        .text("token", token.to_string())
        .text("delete", "");
    
    let response = client
        .post(url)
        .multipart(form)
        .send()
        .await?;
    
    if !response.status().is_success() {
        let status = response.status();
        return Err(anyhow::anyhow!("Delete failed: {}", status));
    }
    
    Ok(())
}
