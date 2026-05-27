use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

/// Matches the terminal-agents.json schema.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub requires: String,
    /// Binary to probe with `which`/`where`. Defaults to first word of launch_cmd.
    #[serde(default)]
    pub binary: String,
    pub launch_cmd: String,
    #[serde(default)]
    pub install_cmd: String,
    #[serde(default)]
    pub docs_url: String,
}

impl AgentConfig {
    /// Returns the binary name to check on PATH.
    pub fn probe_binary(&self) -> &str {
        if !self.binary.is_empty() {
            &self.binary
        } else {
            self.launch_cmd.split_whitespace().next().unwrap_or("unknown")
        }
    }
}

pub const DEFAULT_AGENTS_JSON: &str = r#"[
  {
    "id": "gemini-cli",
    "name": "Gemini CLI",
    "description": "Google's multimodal AI agent. Free tier available with a Google account.",
    "requires": "Node.js 18+",
    "binary": "npx",
    "launch_cmd": "npx @google/gemini-cli",
    "install_cmd": "npx @google/gemini-cli  # no install needed",
    "docs_url": "https://github.com/google/gemini-cli"
  },
  {
    "id": "claude-code",
    "name": "Claude Code",
    "description": "Anthropic's agentic coding tool. Reads files, runs commands, edits code across your project.",
    "requires": "Node.js 18+ • Anthropic account",
    "binary": "claude",
    "launch_cmd": "claude",
    "install_cmd": "npm install -g @anthropic-ai/claude-code",
    "docs_url": "https://docs.anthropic.com/en/docs/claude-code"
  },
  {
    "id": "openai-codex",
    "name": "OpenAI Codex",
    "description": "OpenAI's lightweight terminal coding agent. Requires OPENAI_API_KEY.",
    "requires": "Node.js 18+ • OPENAI_API_KEY",
    "binary": "npx",
    "launch_cmd": "npx -y @openai/codex",
    "install_cmd": "npx -y @openai/codex  # no install needed",
    "docs_url": "https://github.com/openai/codex"
  },
  {
    "id": "opencode",
    "name": "OpenCode",
    "description": "Open-source TUI agent. Supports OpenAI, Anthropic, Gemini, Ollama and more.",
    "requires": "Node.js 18+ • Modern terminal (true color)",
    "binary": "opencode",
    "launch_cmd": "opencode",
    "install_cmd": "curl -fsSL https://opencode.ai/install | bash",
    "docs_url": "https://opencode.ai/"
  },
  {
    "id": "aider",
    "name": "Aider",
    "description": "AI pair programmer with deep Git integration. Supports 100+ LLMs.",
    "requires": "Python 3.8–3.13 • Git • API key",
    "binary": "aider",
    "launch_cmd": "aider",
    "install_cmd": "python -m pip install aider-install && aider-install",
    "docs_url": "https://aider.chat/"
  },
  {
    "id": "open-interpreter",
    "name": "Open Interpreter",
    "description": "Executes code locally to complete tasks. Supports local models via Ollama.",
    "requires": "Python 3.10–3.11 • API key or Ollama",
    "binary": "interpreter",
    "launch_cmd": "interpreter",
    "install_cmd": "pip install open-interpreter",
    "docs_url": "https://openinterpreter.com/"
  },
  {
    "id": "hermes",
    "name": "Hermes Agent",
    "description": "NousResearch's self-improving autonomous agent. Windows users should use WSL2.",
    "requires": "curl • bash • (WSL2 on Windows)",
    "binary": "hermes",
    "launch_cmd": "hermes",
    "install_cmd": "curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash",
    "docs_url": "https://github.com/NousResearch/hermes-agent"
  },
  {
    "id": "plandex",
    "name": "Plandex",
    "description": "Multi-file AI coding agent. Excels at large engineering tasks. Windows requires WSL.",
    "requires": "Linux / macOS / WSL2 • API key",
    "binary": "plandex",
    "launch_cmd": "plandex",
    "install_cmd": "curl -sL https://plandex.ai/install.sh | bash",
    "docs_url": "https://plandex.ai/"
  }
]
"#;

/// Returns the platform-appropriate path to terminal-agents.json.
pub fn config_path() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("~"));
        base.join("nir").join("terminal-agents.json")
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config")
            });
        base.join("nir").join("terminal-agents.json")
    }
}

pub struct LoadedConfig {
    pub agents: Vec<AgentConfig>,
    pub error: Option<String>,
    pub modified: Option<SystemTime>,
}

/// Reads terminal-agents.json, creating defaults on first run.
/// Silently skips entries missing `name` or `launch_cmd`.
pub fn load_config() -> LoadedConfig {
    let path = config_path();

    // Write defaults on first run
    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, DEFAULT_AGENTS_JSON);
    }

    let modified = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());

    match std::fs::read_to_string(&path) {
        Err(e) => LoadedConfig {
            agents: vec![],
            error: Some(format!("Could not read terminal-agents.json: {e}")),
            modified,
        },
        Ok(content) => match serde_json::from_str::<Vec<AgentConfig>>(&content) {
            Err(e) => LoadedConfig {
                agents: vec![],
                error: Some(format!(
                    "terminal-agents.json has a syntax error: {e} — fix and save to reload"
                )),
                modified,
            },
            Ok(mut configs) => {
                configs.retain(|c| !c.name.is_empty() && !c.launch_cmd.is_empty());
                LoadedConfig { agents: configs, error: None, modified }
            }
        },
    }
}
