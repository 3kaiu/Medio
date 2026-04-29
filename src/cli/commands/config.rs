use std::path::Path;

pub fn run_init(config_path: &Path) {
    println!("medio config init — Interactive Configuration Wizard");
    println!();

    // Ensure config directory exists
    if let Some(parent) = config_path.parent()
        && !parent.exists()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!("Error creating config directory: {e}");
        return;
    }

    // Load existing or default config
    let mut config = crate::core::config::AppConfig::load_or_default().unwrap_or_default();

    // TMDB
    println!("1. TMDB API Key (for movie/TV metadata)");
    println!("   Get one at: https://www.themoviedb.org/settings/api");
    let tmdb_key = prompt_input("   Key (or Enter to skip): ");
    if !tmdb_key.is_empty() {
        config.api.tmdb_key = tmdb_key;
    }

    // MusicBrainz
    println!();
    println!("2. MusicBrainz User Agent (for music metadata)");
    let mb_ua = prompt_input("   User agent (or Enter to skip): ");
    if !mb_ua.is_empty() {
        config.api.musicbrainz_user_agent = mb_ua;
    }

    // AI
    println!();
    println!("3. AI Provider (for smart identification)");
    println!("   Options: deepseek / cloudflare / custom / none");
    let ai_choice = prompt_input("   Provider (default: deepseek): ").to_lowercase();
    match ai_choice.as_str() {
        "deepseek" | "" => {
            let key = prompt_input("   DeepSeek API key: ");
            if !key.is_empty() {
                config.ai.deepseek.key = key;
            }
            let model = prompt_input("   Model (default: deepseek-chat): ");
            if !model.is_empty() {
                config.ai.deepseek.model = model;
            }
        }
        "cloudflare" => {
            let account_id = prompt_input("   Cloudflare account ID: ");
            let token = prompt_input("   Cloudflare API token: ");
            if !account_id.is_empty() {
                config.ai.cloudflare.account_id = account_id;
            }
            if !token.is_empty() {
                config.ai.cloudflare.api_token = token;
            }
            let model = prompt_input("   Model (default: @cf/meta/llama-3.1-8b-instruct): ");
            if !model.is_empty() {
                config.ai.cloudflare.model = model;
            }
        }
        "custom" => {
            let url = prompt_input("   API URL: ");
            let key = prompt_input("   API key: ");
            let model = prompt_input("   Model: ");
            config.ai.custom.url = url;
            config.ai.custom.key = key;
            config.ai.custom.model = model;
        }
        "none" => {
            config.ai.enabled = false;
        }
        _ => {}
    }

    // Organize
    println!();
    println!("4. Organize root directory (for archive mode)");
    let root = prompt_input("   Path (or Enter for current directory): ");
    if !root.is_empty() {
        config.organize.root = std::path::PathBuf::from(&root);
    }

    // Operation log
    println!();
    println!("5. Operation log");
    println!("   Record file operations to a local log file");
    let op_log = prompt_input(&format!(
        "   Enable operation log? [Y/n] (current: {}): ",
        if config.general.operation_log {
            "on"
        } else {
            "off"
        }
    ));
    match op_log.to_lowercase().as_str() {
        "" | "y" | "yes" => config.general.operation_log = true,
        "n" | "no" => config.general.operation_log = false,
        _ => {}
    }

    // Save
    println!();
    match config.save() {
        Ok(()) => println!("Config saved to {}", config_path.display()),
        Err(e) => eprintln!("Error saving config: {e}"),
    }
}

fn prompt_input(prompt: &str) -> String {
    use std::io::{self, Write};
    print!("{prompt}");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    input.trim().to_string()
}
