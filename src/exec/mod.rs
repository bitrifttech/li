use anyhow::{Context, Result, anyhow, bail};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;

use crate::agent::ExecutionReport;
use crate::planner::Plan;

/// Execute all dry-run and execute commands in the plan, streaming output to stdout/stderr.
pub async fn execute_plan(plan: &Plan) -> Result<()> {
    println!("\n=== Executing Plan ===");

    if !plan.dry_run_commands.is_empty() {
        println!("\n[Dry-run Phase]");
        for (idx, cmd) in plan.dry_run_commands.iter().enumerate() {
            println!(
                "\n> Running check {}/{}: {}",
                idx + 1,
                plan.dry_run_commands.len(),
                cmd
            );
            let success = run_command(cmd).await?;
            if !success {
                bail!("Dry-run check failed: {}", cmd);
            }
        }
        println!("\n✓ All dry-run checks passed.");
    }

    if !plan.execute_commands.is_empty() {
        println!("\n[Execute Phase]");
        for (idx, cmd) in plan.execute_commands.iter().enumerate() {
            println!(
                "\n> Executing {}/{}: {}",
                idx + 1,
                plan.execute_commands.len(),
                cmd
            );
            let success = run_command(cmd).await?;
            if !success {
                bail!("Command failed: {}", cmd);
            }
        }
        println!("\n✓ Plan execution completed.");
    }

    Ok(())
}

/// Execute the plan and capture combined output for downstream explanation.
pub async fn execute_plan_with_capture(plan: &Plan) -> Result<String> {
    use std::process::Command;

    println!("\n=== Executing Plan ===");
    let mut all_output = String::new();

    if !plan.dry_run_commands.is_empty() {
        println!("\n[Dry-run Phase]");
        all_output.push_str("[Dry-run Phase]\n");

        for (idx, cmd) in plan.dry_run_commands.iter().enumerate() {
            println!(
                "\n> Running check {}/{}: {}",
                idx + 1,
                plan.dry_run_commands.len(),
                cmd
            );
            all_output.push_str(&format!("\nCommand: {}\n", cmd));

            let output = Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .output()
                .context("Failed to execute dry-run command")?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if !stdout.trim().is_empty() {
                println!("\n┌─ COMMAND OUTPUT: {}", cmd);
                println!("│");
                for line in stdout.lines() {
                    println!("│ {}", line);
                }
                println!("│");
                all_output.push_str(&stdout);
            }

            if !stderr.trim().is_empty() {
                eprintln!("│");
                for line in stderr.lines() {
                    eprintln!("│ {}", line);
                }
                all_output.push_str(&stderr);
            }

            if output.status.success() {
                println!("└─ Command completed successfully");
            } else {
                println!(
                    "└─ Command failed with exit code {:?}",
                    output.status.code()
                );
                bail!("Dry-run check failed: {}", cmd);
            }
        }
        println!("\n✓ All dry-run checks passed.");
        all_output.push_str("\n✓ All dry-run checks passed.\n");
    }

    if !plan.execute_commands.is_empty() {
        println!("\n[Execute Phase]");
        all_output.push_str("\n[Execute Phase]\n");

        for (idx, cmd) in plan.execute_commands.iter().enumerate() {
            println!(
                "\n> Executing {}/{}: {}",
                idx + 1,
                plan.execute_commands.len(),
                cmd
            );
            all_output.push_str(&format!("\nCommand: {}\n", cmd));

            let output = Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .output()
                .context("Failed to execute command")?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if !stdout.trim().is_empty() {
                println!("\n┌─ COMMAND OUTPUT: {}", cmd);
                println!("│");
                for line in stdout.lines() {
                    println!("│ {}", line);
                }
                println!("│");
                all_output.push_str(&stdout);
            }

            if !stderr.trim().is_empty() {
                eprintln!("│");
                for line in stderr.lines() {
                    eprintln!("│ {}", line);
                }
                all_output.push_str(&stderr);
            }

            if output.status.success() {
                println!("└─ Command completed successfully");
            } else {
                println!(
                    "└─ Command failed with exit code {:?}",
                    output.status.code()
                );
                bail!("Command failed: {}", cmd);
            }
        }
        println!("\n✓ Plan execution completed.");
        all_output.push_str("\n✓ Plan execution completed.\n");
    }

    Ok(all_output)
}

/// Execute the plan and return a structured report without emitting additional notes.
pub async fn execution_report(plan: &Plan) -> Result<ExecutionReport> {
    let output = execute_plan_with_capture(plan).await?;
    Ok(ExecutionReport {
        commands: plan.execute_commands.clone(),
        success: true,
        stdout: if output.trim().is_empty() {
            None
        } else {
            Some(output)
        },
        stderr: None,
        notes: Vec::new(),
    })
}

/// Run a shell command, streaming output to stdout/stderr.
pub async fn run_command(cmd: &str) -> Result<bool> {
    let modified_cmd = if cmd.starts_with("ls ") || cmd == "ls" {
        cmd.replace("ls", "ls --color=always")
    } else {
        cmd.to_string()
    };

    println!("\n┌─ COMMAND OUTPUT: {}", cmd);
    println!("│");

    let mut child = TokioCommand::new("sh")
        .arg("-c")
        .arg(&modified_cmd)
        .env("FORCE_COLOR", "1")
        .env("CLICOLOR_FORCE", "1")
        .env("COLORTERM", "truecolor")
        .env("TERM", "xterm-256color")
        .env("GIT_CONFIG_PARAMETERS", "'color.ui=always'")
        .env("LS_COLORS", "di=1;34:fi=0:ln=1;36:pi=40;33:so=1;35:do=1;35:bd=40;33;01:cd=40;33;01:or=40;31;01:ex=1;32:*.tar=1;31:*.tgz=1;31:*.zip=1;31:*.gz=1;31:*.bz2=1;31:*.deb=1;31:*.rpm=1;31:*.jpg=1;35:*.png=1;35:*.gif=1;35:*.bmp=1;35:*.ppm=1;35:*.tga=1;35:*.xbm=1;35:*.xpm=1;35:*.tif=1;35:*.mpg=1;37:*.avi=1;37:*.gl=1;37:*.dl=1;37:*.jpg=1;35:*.png=1;35:*.gif=1;35:*.bmp=1;35:*.ppm=1;35:*.tga=1;35:*.xbm=1;35:*.xpm=1;35:*.tif=1;35:")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow!(
                    "Command not found: {}. Please ensure the command exists in your PATH.",
                    cmd
                )
            } else {
                anyhow!("Failed to execute command '{}': {}", cmd, e)
            }
        })?;

    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let stderr = child.stderr.take().expect("Failed to capture stderr");

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let stdout_handle = tokio::spawn(async move {
        while let Ok(Some(line)) = stdout_reader.next_line().await {
            println!("│ {}", line);
        }
    });

    let stderr_handle = tokio::spawn(async move {
        while let Ok(Some(line)) = stderr_reader.next_line().await {
            eprintln!("│ {}", line);
        }
    });

    let status = child
        .wait()
        .await
        .map_err(|e| anyhow!("Failed to wait for command completion: {}", e))?;

    stdout_handle
        .await
        .map_err(|e| anyhow!("Failed to read command output: {}", e))?;
    stderr_handle
        .await
        .map_err(|e| anyhow!("Failed to read command errors: {}", e))?;

    if status.success() {
        println!("│");
        println!("└─ Command completed successfully");
        Ok(true)
    } else {
        println!("│");
        if let Some(code) = status.code() {
            println!("└─ Command failed with exit code {}", code);
            Err(anyhow!("Command failed with exit code {}: {}", code, cmd))
        } else {
            println!("└─ Command was terminated by signal");
            Err(anyhow!("Command was terminated by signal: {}", cmd))
        }
    }
}
