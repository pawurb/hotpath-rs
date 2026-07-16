use std::io::ErrorKind;
use std::process::Command;

const SKILL_URL_TEMPLATE: &str =
    "https://raw.githubusercontent.com/pawurb/hotpath-rs/v{version}/skills/hotpath_init/SKILL.md";

const KICKOFF_PROMPT: &str = "Set up hotpath profiling in this repo.";

#[derive(Debug, Clone, Copy)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub fn from_arg(arg: Option<&str>) -> Result<Self, String> {
        match arg {
            None | Some("claude") => Ok(Self::Claude),
            Some("codex") => Ok(Self::Codex),
            Some(other) => Err(format!(
                "Unknown agent '{other}'. Supported agents: claude, codex"
            )),
        }
    }

    fn bin(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

pub fn run(agent: Agent) -> Result<(), String> {
    let url = skill_url();
    println!("Downloading setup instructions from {url}");
    let skill = download_skill(&url)?;
    let instructions = strip_frontmatter(&skill);

    println!(
        "Starting {} session with hotpath setup instructions...",
        agent.bin()
    );

    let mut command = Command::new(agent.bin());
    match agent {
        Agent::Claude => {
            command
                .arg("--append-system-prompt")
                .arg(instructions)
                .arg(KICKOFF_PROMPT);
        }
        Agent::Codex => {
            command.arg(format!(
                "{instructions}\n\n{KICKOFF_PROMPT} Follow the instructions above."
            ));
        }
    }

    let status = command.status().map_err(|e| {
        if e.kind() == ErrorKind::NotFound {
            format!("'{}' not found. Is it installed and on PATH?", agent.bin())
        } else {
            format!("Failed to launch '{}': {e}", agent.bin())
        }
    })?;

    if !status.success() {
        return Err(format!(
            "{} session exited with status: {status}",
            agent.bin()
        ));
    }

    Ok(())
}

fn skill_url() -> String {
    SKILL_URL_TEMPLATE.replace("{version}", env!("CARGO_PKG_VERSION"))
}

fn download_skill(url: &str) -> Result<String, String> {
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", "10", url])
        .output()
        .map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                "'curl' not found. Install curl to use 'hotpath init'.".to_string()
            } else {
                format!("Failed to run curl: {e}")
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Failed to download setup instructions from {url}: {}",
            stderr.trim()
        ));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| format!("Setup instructions are not valid UTF-8: {e}"))
}

fn strip_frontmatter(skill: &str) -> &str {
    let Some(rest) = skill.strip_prefix("---\n") else {
        return skill;
    };
    match rest.find("\n---\n") {
        Some(end) => rest[end + "\n---\n".len()..].trim_start(),
        None => skill,
    }
}

#[cfg(test)]
mod tests {
    use super::strip_frontmatter;

    #[test]
    fn strips_yaml_frontmatter() {
        let skill = "---\nname: hotpath_init\ndescription: Configure hotpath.\n---\n\n# Initialize\n\nBody text.";
        assert_eq!(strip_frontmatter(skill), "# Initialize\n\nBody text.");
    }

    #[test]
    fn returns_input_without_frontmatter() {
        let skill = "# Initialize\n\nBody text.";
        assert_eq!(strip_frontmatter(skill), skill);
    }

    #[test]
    fn returns_input_with_unterminated_frontmatter() {
        let skill = "---\nname: hotpath_init\nno closing marker";
        assert_eq!(strip_frontmatter(skill), skill);
    }
}
