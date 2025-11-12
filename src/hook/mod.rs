use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};

const HOOK_SCRIPT: &str = r#"# li natural language terminal hook
_li_accept_line() {
    local input="$BUFFER"

    if [[ -z "$input" ]]; then
        zle .accept-line
        return
    fi

    local output
    output="$(li --classify "$input" 2>&1)"
    local code=$?

    if [[ $code -eq 100 ]]; then
        zle .accept-line
    elif [[ $code -eq 0 ]]; then
        zle .kill-whole-line
        local escaped_input=${(q)input}
        print -z "li $escaped_input"
        zle .accept-line
    else
        echo "$output" >&2
        echo "li classification error (status $code)" >&2
        zle .accept-line
    fi
}

zle -N _li_accept_line
bindkey '^M' _li_accept_line
"#;

const SOURCE_LINE: &str = "source ~/.zshrc.d/li.zsh";

#[allow(dead_code)]
pub fn install() -> Result<()> {
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow!("Unable to determine the current home directory"))?;
    let hook_path = home.join(".zshrc.d").join("li.zsh");

    ensure_parent_dir(&hook_path)?;
    write_hook(&hook_path)?;

    let zshrc_path = home.join(".zshrc");
    ensure_source_line(&zshrc_path)?;

    println!("✓ Installed zsh hook to {}", hook_path.display());
    println!("  Restart your shell or run: source ~/.zshrc");

    Ok(())
}

#[allow(dead_code)]
pub fn uninstall() -> Result<()> {
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow!("Unable to determine the current home directory"))?;
    let hook_path = home.join(".zshrc.d").join("li.zsh");

    if hook_path.exists() {
        fs::remove_file(&hook_path)
            .with_context(|| format!("Failed to remove {}", hook_path.display()))?;
        println!("✓ Removed {}", hook_path.display());
    }

    let zshrc_path = home.join(".zshrc");
    remove_source_line(&zshrc_path)?;

    Ok(())
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Err(anyhow!(
            "Unable to determine parent directory for hook path {}",
            path.display()
        ));
    };

    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create directory {}", parent.display()))
}

fn write_hook(path: &Path) -> Result<()> {
    fs::write(path, HOOK_SCRIPT)
        .with_context(|| format!("Failed to write hook script to {}", path.display()))
}

fn ensure_source_line(zshrc: &Path) -> Result<()> {
    let mut contents = if zshrc.exists() {
        fs::read_to_string(zshrc).with_context(|| format!("Failed to read {}", zshrc.display()))?
    } else {
        String::new()
    };

    let already_present = contents.lines().any(|line| line.trim_end() == SOURCE_LINE);
    if !already_present {
        if !contents.is_empty() && !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str(SOURCE_LINE);
        contents.push('\n');

        fs::write(zshrc, contents)
            .with_context(|| format!("Failed to update {}", zshrc.display()))?;
    }

    Ok(())
}

fn remove_source_line(zshrc: &Path) -> Result<()> {
    if !zshrc.exists() {
        return Ok(());
    }

    let original =
        fs::read_to_string(zshrc).with_context(|| format!("Failed to read {}", zshrc.display()))?;
    let filtered: Vec<&str> = original
        .lines()
        .filter(|line| line.trim_end() != SOURCE_LINE)
        .collect();

    let mut new_contents = filtered.join("\n");
    if original.ends_with('\n') && !new_contents.is_empty() {
        new_contents.push('\n');
    }

    if new_contents != original {
        fs::write(zshrc, new_contents)
            .with_context(|| format!("Failed to update {}", zshrc.display()))?;
        println!("✓ Removed hook from {}", zshrc.display());
    }

    Ok(())
}
