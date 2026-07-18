use std::io::ErrorKind;
use std::process::Command;

const SKILL_URL_BRANCH_TEMPLATE: &str =
    "https://raw.githubusercontent.com/pawurb/hotpath-rs/init-v{minor}/skills/hotpath_init/SKILL.md";
const SKILL_URL_TAG_TEMPLATE: &str =
    "https://raw.githubusercontent.com/pawurb/hotpath-rs/v{version}/skills/hotpath_init/SKILL.md";

const KICKOFF_PROMPT: &str = "Set up hotpath profiling in this repo.";

#[derive(Debug, Clone, Copy)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub fn from_arg(arg: &str) -> Result<Self, String> {
        match arg {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            other => Err(format!(
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
    let branch_url = branch_skill_url();
    println!("Downloading setup instructions from {branch_url}");
    let skill = match download_skill(&branch_url) {
        Ok(skill) => skill,
        Err(branch_err) => {
            let tag_url = tag_skill_url();
            println!("Branch download failed, retrying from {tag_url}");
            download_skill(&tag_url).map_err(|tag_err| format!("{branch_err}\n{tag_err}"))?
        }
    };
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

fn branch_skill_url() -> String {
    SKILL_URL_BRANCH_TEMPLATE.replace("{minor}", minor_version(env!("CARGO_PKG_VERSION")))
}

fn tag_skill_url() -> String {
    SKILL_URL_TAG_TEMPLATE.replace("{version}", env!("CARGO_PKG_VERSION"))
}

fn minor_version(version: &str) -> &str {
    version.rsplit_once('.').map_or(version, |(minor, _)| minor)
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
    use super::{minor_version, strip_frontmatter, SKILL_URL_BRANCH_TEMPLATE};

    #[test]
    fn derives_minor_version() {
        assert_eq!(minor_version("0.21.4"), "0.21");
        assert_eq!(minor_version("1.0.0"), "1.0");
        assert_eq!(minor_version("0.21"), "0");
    }

    #[test]
    fn branch_url_uses_minor_version() {
        let url = SKILL_URL_BRANCH_TEMPLATE.replace("{minor}", minor_version("0.21.4"));
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/pawurb/hotpath-rs/init-v0.21/skills/hotpath_init/SKILL.md"
        );
    }

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
