use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use chrono::{Utc, TimeZone, Datelike};
use regex::Regex;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

use gpui::{App, Entity, SharedString, Task};
use project::Project;
use agent_client_protocol::schema as acp;

use crate::{AgentTool, ToolCallEventStream, ToolInput};
use super::brain_memory_tool::{brain_dir, memory_dir};

const CREATE_RETENTION_DAYS: u32 = 2;

fn backup_dir() -> PathBuf {
    brain_dir().join("backups")
}

fn drafts_dir() -> PathBuf {
    brain_dir().join("drafts")
}

fn get_today_date() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

fn ensure_dir(dir_path: &Path) {
    if !dir_path.exists() {
        let _ = fs::create_dir_all(dir_path);
    }
}

fn get_target_backup_dir(target: &str) -> PathBuf {
    backup_dir().join(target)
}

struct ParsedBackupName {
    date: String,
    target: String,
}

fn parse_backup_name(name: &str) -> Option<ParsedBackupName> {
    let re_file = Regex::new(r"^(\d{4}-\d{2}-\d{2})-(?:\d+-)?(about\.md|goals\.md|settings\.md|bookmark\.md|projects\.md)$").unwrap();
    if let Some(caps) = re_file.captures(name) {
        return Some(ParsedBackupName {
            date: caps[1].to_string(),
            target: caps[2].replace(".md", ""),
        });
    }

    let re_drafts = Regex::new(r"^(\d{4}-\d{2}-\d{2})-(?:\d+-)?drafts$").unwrap();
    if let Some(caps) = re_drafts.captures(name) {
        return Some(ParsedBackupName {
            date: caps[1].to_string(),
            target: "drafts".to_string(),
        });
    }

    None
}

fn build_unique_backup_path(dir_path: &Path, base_name: &str) -> PathBuf {
    let mut candidate = dir_path.join(base_name);
    if !candidate.exists() {
        return candidate;
    }

    let prefix_re = Regex::new(r"^(\d{4}-\d{2}-\d{2})-(.+)$").unwrap();
    let mut counter = 0;

    while candidate.exists() {
        let stamp = Utc::now().timestamp_millis() + counter;
        let stamped_name = if let Some(caps) = prefix_re.captures(base_name) {
            format!("{}-{}-{}", &caps[1], stamp, &caps[2])
        } else {
            format!("{}-{}", stamp, base_name)
        };
        candidate = dir_path.join(stamped_name);
        counter += 1;
    }
    candidate
}

struct BackupEntry {
    target: String,
    name: String,
    full_path: PathBuf,
    is_directory: bool,
    date: String,
}

fn collect_backup_entries_from_dir(dir_path: &Path, only_target: Option<&str>) -> Vec<BackupEntry> {
    let mut entries = Vec::new();
    if !dir_path.exists() {
        return entries;
    }

    if let Ok(dir_entries) = fs::read_dir(dir_path) {
        for entry in dir_entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(parsed) = parse_backup_name(&name) {
                if let Some(t) = only_target {
                    if parsed.target != t {
                        continue;
                    }
                }
                let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
                entries.push(BackupEntry {
                    target: parsed.target,
                    name,
                    full_path: entry.path(),
                    is_directory: is_dir,
                    date: parsed.date,
                });
            }
        }
    }
    entries
}

fn collect_all_backup_entries() -> Vec<BackupEntry> {
    let all_targets = ["about", "goals", "settings", "bookmark", "projects", "drafts"];
    let mut entries = Vec::new();

    for target in all_targets {
        let mut t_entries = collect_backup_entries_from_dir(&get_target_backup_dir(target), Some(target));
        entries.append(&mut t_entries);
    }

    let mut legacy = collect_backup_entries_from_dir(&backup_dir(), None);
    entries.append(&mut legacy);

    entries.sort_by(|a, b| b.name.cmp(&a.name));
    entries
}

fn prune_old_backups_for_target(target: &str, keep_days: u32) -> usize {
    let cutoff = if keep_days == 0 {
        None
    } else {
        let c = Utc::now() - chrono::Duration::days((keep_days - 1) as i64);
        Some(Utc.with_ymd_and_hms(c.year(), c.month(), c.day(), 0, 0, 0).unwrap())
    };
    
    let entries = collect_all_backup_entries();
    let mut pruned = 0;

    for entry in entries {
        if entry.target != target {
            continue;
        }

        if let Ok(entry_date) = chrono::NaiveDateTime::parse_from_str(&format!("{} 00:00:00", entry.date), "%Y-%m-%d %H:%M:%S").map(|dt| dt.and_utc()) {
            if let Some(c) = cutoff {
                if entry_date >= c {
                    continue;
                }
            }
            if entry.is_directory {
                let _ = fs::remove_dir_all(&entry.full_path);
            } else {
                let _ = fs::remove_file(&entry.full_path);
            }
            pruned += 1;
        }
    }
    pruned
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

fn resolve_backup_entry(backup_ref: &str) -> Result<BackupEntry, String> {
    if backup_ref.contains('/') || backup_ref.contains('\\') {
        let normalized = backup_ref.replace('\\', "/");
        let full_path = backup_dir().join(&normalized);
        if !full_path.exists() {
            return Err(format!("Error: backup '{}' not found.", backup_ref));
        }
        let name = full_path.file_name().unwrap().to_string_lossy().to_string();
        if let Some(parsed) = parse_backup_name(&name) {
            let is_dir = full_path.is_dir();
            return Ok(BackupEntry {
                target: parsed.target,
                name,
                full_path,
                is_directory: is_dir,
                date: parsed.date,
            });
        }
        return Err(format!("Error: backup '{}' does not match naming format.", backup_ref));
    }

    let all_entries = collect_all_backup_entries();
    let matches: Vec<_> = all_entries.into_iter().filter(|e| e.name == backup_ref).collect();

    if matches.is_empty() {
        return Err(format!("Error: backup '{}' not found.", backup_ref));
    }
    if matches.len() > 1 {
        let options = matches.iter().map(|m| format!("{}/{}", m.target, m.name)).collect::<Vec<_>>().join("\n");
        return Err(format!("Error: backup name '{}' is ambiguous. Use one of these:\n{}", backup_ref, options));
    }

    Ok(matches.into_iter().next().unwrap())
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BackupAction {
    #[default]
    Create,
    List,
    Restore,
    Prune,
    Delete,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BackupTargetEnum {
    #[default]
    All,
    About,
    Goals,
    Settings,
    Bookmark,
    Projects,
    Drafts,
}

impl BackupTargetEnum {
    fn as_str(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::About => "about",
            Self::Goals => "goals",
            Self::Settings => "settings",
            Self::Bookmark => "bookmark",
            Self::Projects => "projects",
            Self::Drafts => "drafts",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BackupInput {
    #[serde(default)]
    action: BackupAction,
    #[serde(default)]
    target: BackupTargetEnum,
    backup_file: Option<String>,
    keep_days: Option<u32>,
}

pub struct BackupTool {
    _project: Entity<Project>,
}

impl BackupTool {
    pub fn new(project: Entity<Project>) -> Self {
        Self { _project: project }
    }
}

impl AgentTool for BackupTool {
    type Input = BackupInput;
    type Output = String;

    const NAME: &'static str = "brain_backup";

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(
        &self,
        _input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        "Managing brain backups".into()
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        _event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<String, String>> {
        cx.spawn(async move |_cx| {
            let input = input.recv().await.map_err(|e| format!("Failed to receive input: {e}"))?;

            ensure_dir(&backup_dir());
            let all_targets = ["about", "goals", "settings", "bookmark", "projects", "drafts"];
            for t in all_targets {
                ensure_dir(&get_target_backup_dir(t));
            }

            let today = get_today_date();

            if let BackupAction::Create = input.action {
                let targets = if input.target.as_str() == "all" {
                    vec!["about.md", "goals.md", "settings.md", "bookmark.md", "projects.md", "drafts"]
                } else if input.target.as_str() == "drafts" {
                    vec!["drafts"]
                } else {
                    vec![input.target.as_str()]
                };

                let mut results = Vec::new();
                for target_req in targets {
                    let target = if target_req == "drafts" {
                        "drafts".to_string()
                    } else if target_req.ends_with(".md") {
                        target_req.replace(".md", "")
                    } else {
                        target_req.to_string()
                    };
                    
                    let filename = if target == "drafts" { "drafts".to_string() } else { format!("{}.md", target) };
                    let target_dir = get_target_backup_dir(&target);

                    let removed = prune_old_backups_for_target(&target, CREATE_RETENTION_DAYS);
                    if removed > 0 {
                        results.push(format!("Pruned {} old backup(s) for {}/ older than {} days.", removed, target, CREATE_RETENTION_DAYS));
                    }

                    if filename == "drafts" {
                        if !drafts_dir().exists() {
                            results.push("Skipped drafts — folder not found.".to_string());
                            continue;
                        }
                        let backup_name = format!("{}-drafts", today);
                        let final_dest = build_unique_backup_path(&target_dir, &backup_name);
                        if let Ok(_) = copy_dir_recursive(&drafts_dir(), &final_dest) {
                            results.push(format!("Backed up drafts folder -> drafts/{}", final_dest.file_name().unwrap().to_string_lossy()));
                        }
                        continue;
                    }

                    let src_path = memory_dir().join(&filename);
                    if !src_path.exists() {
                        results.push(format!("Skipped {} — file not found.", filename));
                        continue;
                    }

                    let backup_name = format!("{}-{}", today, filename);
                    let final_dest = build_unique_backup_path(&target_dir, &backup_name);
                    if let Ok(_) = fs::copy(&src_path, &final_dest) {
                        results.push(format!("Backed up {} -> {}/{}", filename, target, final_dest.file_name().unwrap().to_string_lossy()));
                    }
                }
                return Ok(results.join("\n"));
            }

            if let BackupAction::List = input.action {
                let entries = collect_all_backup_entries();
                if entries.is_empty() {
                    return Ok("No backups found yet.".to_string());
                }
                let lines: Vec<String> = entries.iter().map(|e| format!("{}/{}", e.target, e.name)).collect();
                return Ok(format!("Available backups ({}):\n{}", entries.len(), lines.join("\n")));
            }

            if let BackupAction::Restore = input.action {
                let backup_file = input.backup_file.ok_or_else(|| "Error: backup_file is required for restore.".to_string())?;
                let resolved = resolve_backup_entry(&backup_file)?;

                if resolved.target == "drafts" {
                    if !resolved.is_directory {
                        return Err(format!("Error: drafts backup '{}' is not a valid backup folder.", resolved.name));
                    }
                    if drafts_dir().exists() {
                        let safety_dir = get_target_backup_dir("drafts");
                        let final_safety = build_unique_backup_path(&safety_dir, &format!("{}-pre-restore-drafts", today));
                        let _ = copy_dir_recursive(&drafts_dir(), &final_safety);
                        let _ = fs::remove_dir_all(&drafts_dir());
                    }
                    let _ = copy_dir_recursive(&resolved.full_path, &drafts_dir());
                    return Ok(format!("Restored drafts folder from {}/{}.", resolved.target, resolved.name));
                }

                if resolved.is_directory {
                    return Err(format!("Error: backup '{}/{}' is a directory. Expected a file backup.", resolved.target, resolved.name));
                }

                let target_filename = format!("{}.md", resolved.target);
                let target_path = memory_dir().join(&target_filename);

                if target_path.exists() {
                    let safety_dir = get_target_backup_dir(&resolved.target);
                    let safety_backup = build_unique_backup_path(&safety_dir, &format!("{}-pre-restore-{}", today, target_filename));
                    let _ = fs::copy(&target_path, &safety_backup);
                }

                let _ = fs::copy(&resolved.full_path, &target_path);
                return Ok(format!("Restored {} from {}/{}.", target_filename, resolved.target, resolved.name));
            }

            if let BackupAction::Prune = input.action {
                let keep_days = input.keep_days.unwrap_or(30);
                if !backup_dir().exists() {
                    return Ok("No backup directory found.".to_string());
                }
                let mut pruned = 0;
                for t in all_targets {
                    pruned += prune_old_backups_for_target(t, keep_days);
                }
                if pruned > 0 {
                    return Ok(format!("Pruned {} backup(s) older than {} days.", pruned, keep_days));
                } else {
                    return Ok(format!("No backups older than {} days found.", keep_days));
                }
            }

            if let BackupAction::Delete = input.action {
                let backup_file = input.backup_file.ok_or_else(|| "Error: backup_file is required for delete.".to_string())?;
                let resolved = resolve_backup_entry(&backup_file)?;
                
                if resolved.is_directory {
                    let _ = fs::remove_dir_all(&resolved.full_path);
                } else {
                    let _ = fs::remove_file(&resolved.full_path);
                }
                return Ok(format!("Deleted backup {}/{}.", resolved.target, resolved.name));
            }

            Err("Invalid action.".to_string())
        })
    }
}

