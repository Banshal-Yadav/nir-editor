use serde::{Deserialize, Serialize};
use agent_skills::{SKILL_FILE_NAME, global_skills_dir};
use std::collections::HashSet;
use anyhow::{Result, Context};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Metrics {
    pub complexity_score: u32,
    pub had_error_recovery: bool,
    pub user_corrections_count: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StagedRecollection {
    pub id: String,
    pub category: String,
    pub associated_summaries: Vec<String>,
    pub metrics: Metrics,
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct RecollectionsRegistry {
    pub staged_recollections: Vec<StagedRecollection>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SkillIndexEntry {
    pub name: String,
    pub description: String,
    pub last_used_timestamp: i64,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct SkillsIndex {
    pub active_skills: Vec<SkillIndexEntry>,
    #[serde(default)]
    pub discovered_skills: Vec<SkillIndexEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SkillPayload {
    pub name: String,
    pub description: String,
    pub system_instruction_override: String,
}

// Windows shells can isolate USERPROFILE vs HOME inconsistently across PowerShell/WSL/MSYS;
// strict ordering + `.` fallback prevents telemetry from fragmenting to silent directories.
fn home_dir_path() -> PathBuf {
    if let Ok(path) = std::env::var("USERPROFILE") {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("HOME") {
        return PathBuf::from(path);
    }
    PathBuf::from(".")
}

impl RecollectionsRegistry {
    /// Calculate basic token intersection similarity (0.0 to 1.0) to avoid heavy libraries
    pub fn calculate_similarity(s1: &str, s2: &str) -> f32 {
        let clean_tokens = |s: &str| -> HashSet<String> {
            s.to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|t| t.len() > 2) // Ignore short fluff tokens
                .map(|t| t.to_string())
                .collect()
        };

        let set1 = clean_tokens(s1);
        let set2 = clean_tokens(s2);

        if set1.is_empty() || set2.is_empty() {
            return 0.0;
        }

        let intersection_count = set1.intersection(&set2).count();
        let min_size = std::cmp::min(set1.len(), set2.len());
        intersection_count as f32 / min_size as f32
    }

    /// Process a newly ingested checkpoint into the current registry state
    pub fn merge_checkpoint(&mut self, checkpoint: ParsedCheckpoint) {
        let mut matched_index: Option<usize> = None;

        // Try to match against existing recollections in the same category
        for (i, staged) in self.staged_recollections.iter().enumerate() {
            if staged.category.to_uppercase() != checkpoint.category.to_uppercase() {
                continue;
            }

            // Check similarity against existing items in this cluster
            for existing_summary in &staged.associated_summaries {
                if Self::calculate_similarity(existing_summary, &checkpoint.summary) >= 0.40 {
                    matched_index = Some(i);
                    break;
                }
            }
            if matched_index.is_some() { 
                break; 
            }
        }

        if let Some(idx) = matched_index {
            // Update existing cluster metrics and append summary
            let entry = &mut self.staged_recollections[idx];
            if !entry.associated_summaries.contains(&checkpoint.summary) {
                entry.associated_summaries.push(checkpoint.summary.clone());
            }
            if checkpoint.tags.contains(&"error_recovery".to_string()) {
                entry.metrics.had_error_recovery = true;
            }
            if checkpoint.tags.contains(&"user_intervention".to_string()) {
                entry.metrics.user_corrections_count += 1;
            }
        } else {
            // Initialize a completely new behavioral cluster using a short slug from the text
            let slug = checkpoint.summary
                .to_lowercase()
                .split_whitespace()
                .take(3)
                .collect::<Vec<&str>>()
                .join("-")
                .replace(|c: char| !c.is_alphanumeric() && c != '-', "");

            let new_staged = StagedRecollection {
                id: format!("{}-{}", slug, chrono::Utc::now().timestamp_millis() % 1000),
                category: checkpoint.category.clone(),
                metrics: Metrics {
                    complexity_score: 1,
                    had_error_recovery: checkpoint.tags.contains(&"error_recovery".to_string()),
                    user_corrections_count: if checkpoint.tags.contains(&"user_intervention".to_string()) { 1 } else { 0 },
                },
                associated_summaries: vec![checkpoint.summary],
                status: "STAGED".to_string(),
            };
            self.staged_recollections.push(new_staged);
        }
    }

    /// Identifies staged items that have met the threshold for promotion to full skills.
    /// Threshold: 3 or more associated summaries, OR an error recovery with multiple user corrections.
    pub fn check_promotion_targets(&self) -> Vec<StagedRecollection> {
        self.staged_recollections
            .iter()
            .filter(|staged| {
                if staged.status != "STAGED" {
                    return false;
                }
                
                let frequency_met = staged.associated_summaries.len() >= 3;
                let high_friction_met = staged.metrics.had_error_recovery 
                    && staged.metrics.user_corrections_count >= 2;

                frequency_met || high_friction_met
            })
            .cloned()
            .collect()
    }
}

pub struct ParsedCheckpoint {
    pub category: String,
    pub summary: String,
    pub tags: Vec<String>,
}

/// Parses machine-scannable log lines.
/// Accepts both the legacy format `[TIME] [CATEGORY] SUMMARY Tags: [x, y]`
/// and the ID-tagged format written by `log_task_completion`:
/// `[TIME] | ID:xxx | Completed Task: <summary>`.
pub fn process_log_line(line: &str) -> Option<ParsedCheckpoint> {
    if !line.starts_with('[') {
        return None;
    }

    if let Some(checkpoint) = parse_id_tagged_log_line(line) {
        return Some(checkpoint);
    }

    parse_legacy_log_line(line)
}

fn parse_id_tagged_log_line(line: &str) -> Option<ParsedCheckpoint> {
    let close_time_idx = line.find(']')?;
    let after_time = line[close_time_idx + 1..].trim_start();
    if !after_time.starts_with('|') {
        return None;
    }

    let marker = "Completed Task:";
    let marker_idx = line.find(marker)?;
    let summary = line[marker_idx + marker.len()..].trim().to_string();
    if summary.is_empty() {
        return None;
    }

    Some(ParsedCheckpoint {
        category: "task_completion".to_string(),
        summary,
        tags: Vec::new(),
    })
}

fn parse_legacy_log_line(line: &str) -> Option<ParsedCheckpoint> {
    if !line.contains("] [") {
        return None;
    }

    let close_time_idx = line.find(']')?;
    let remainder = &line[close_time_idx + 2..];
    let close_cat_idx = remainder.find(']')?;

    let category = remainder[1..close_cat_idx].to_string();
    let mut main_content = remainder[close_cat_idx + 2..].trim().to_string();

    let mut tags = Vec::new();
    if let Some(tag_start) = main_content.find("Tags: [") {
        if let Some(tag_end) = main_content.find(']') {
            if tag_end > tag_start {
                let tag_segment = &main_content[tag_start + 7..tag_end];
                tags = tag_segment.split(',').map(|t| t.trim().to_string()).collect();
                main_content = main_content[..tag_start].trim().to_string();
            }
        }
    }

    Some(ParsedCheckpoint { category, summary: main_content, tags })
}

pub async fn run_analytics_cycle() -> Result<Vec<String>> {
    let home = home_dir_path();
    let brain_dir = home.join(".nir/brain");
    let recollections_path = brain_dir.join("recollections.json");
    let logs_dir = brain_dir.join("logs");
    let skills_dir = brain_dir.join("skills");
    let index_path = brain_dir.join("skills_index.json");

    let mut registry = if recollections_path.exists() {
        let content = fs::read_to_string(&recollections_path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        RecollectionsRegistry::default()
    };

    let current_date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let target_log = logs_dir.join(format!("{}.md", current_date));

    if target_log.exists() {
        let log_data = fs::read_to_string(&target_log)?;
        for line in log_data.lines() {
            if let Some(checkpoint) = process_log_line(line) {
                registry.merge_checkpoint(checkpoint);
            }
        }
    }

    let eligible_targets = registry.check_promotion_targets();
    let mut promoted_skill_names = Vec::new();

    for target in eligible_targets {
        let mut instruction_override = format!(
            "When working within the '{}' domain, adhere to these established patterns gathered across past sessions:\n",
            target.category
        );
        for summary in &target.associated_summaries {
            instruction_override.push_str(&format!("- {}\n", summary));
        }

        let clean_name = write_promoted_skill(
            &target.category,
            &target.associated_summaries[0],
            &instruction_override,
        ).await?;

        promoted_skill_names.push(clean_name);

        if let Some(registry_item) = registry.staged_recollections.iter_mut().find(|s| s.id == target.id) {
            registry_item.status = "PROMOTED".to_string();
        }
    }

    if index_path.exists() {
        let content = fs::read_to_string(&index_path)?;
        let mut index: SkillsIndex = serde_json::from_str(&content).unwrap_or_default();

        let now = chrono::Utc::now().timestamp();
        let expiration_threshold = 30 * 24 * 60 * 60;
        let mut active_skills = Vec::new();
        let archive_dir = skills_dir.join(".archive");

        for skill in index.active_skills {
            if now - skill.last_used_timestamp > expiration_threshold {
                fs::create_dir_all(&archive_dir).context("Failed to create archive directory")?;
                let filename = to_slug(&skill.name);
                let src_path = skills_dir.join(format!("{}.json", filename));
                let dest_path = archive_dir.join(format!("{}.json", filename));
                
                if src_path.exists() {
                    fs::rename(&src_path, &dest_path)?;
                }
            } else {
                active_skills.push(skill);
            }
        }
        
        index.active_skills = active_skills;
        let serialized_index = serde_json::to_string_pretty(&index)?;
        let mut idx_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&index_path)?;
        idx_file.write_all(serialized_index.as_bytes())?;
    }

    let serialized = serde_json::to_string_pretty(&registry)?;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&recollections_path)?;
    file.write_all(serialized.as_bytes())?;

    Ok(promoted_skill_names)
}

pub fn approve_discovered_skill(name: &str) -> Result<()> {
    let home = home_dir_path();
    let brain_dir = home.join(".nir/brain");
    let index_path = brain_dir.join("skills_index.json");
    let skills_dir = brain_dir.join("skills");

    let mut index = if index_path.exists() {
        let content = fs::read_to_string(&index_path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        SkillsIndex::default()
    };

    let slug = to_slug(name);
    if let Some(pos) = index
        .discovered_skills
        .iter()
        .position(|skill| skill.name == slug)
    {
        let entry = index.discovered_skills.remove(pos);
        if !index.active_skills.iter().any(|skill| skill.name == entry.name) {
            index.active_skills.push(entry);
        }
    }

    let home_dir = home_dir_path();
    let mut markdown_path = home_dir.join(".agents").join("proposals").join(&slug).join("SKILL.md");
    if !markdown_path.exists() {
        markdown_path = skills_dir.join(&slug).join("SKILL.md");
    }
    let content = fs::read_to_string(&markdown_path)?;
    
    let mut description = String::new();
    let mut body = String::new();
    let mut inside_frontmatter = false;
    let mut frontmatter_ended = false;

    for line in content.lines() {
        if line.trim() == "---" {
            if !inside_frontmatter && !frontmatter_ended {
                inside_frontmatter = true;
            } else if inside_frontmatter {
                inside_frontmatter = false;
                frontmatter_ended = true;
            }
            continue;
        }
        if inside_frontmatter {
            if let Some(idx) = line.find(':') {
                let key = line[..idx].trim();
                let val = line[idx + 1..].trim();
                if key == "description" {
                    description = val.to_string();
                }
            }
        } else if frontmatter_ended {
            body.push_str(line);
            body.push('\n');
        }
    }
    let body = body.trim();

    let active_skill_dir = global_skills_dir().join(&slug);
    fs::create_dir_all(&active_skill_dir)?;
    let skill_content = format!(
        "---\nid: {}\nname: {}\ndescription: {}\n---\n{}",
        slug, name, description, body
    );

    let skill_file_path = active_skill_dir.join(SKILL_FILE_NAME);
    let mut skill_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&skill_file_path)?;
    skill_file.write_all(skill_content.as_bytes())?;

    let serialized_index = serde_json::to_string_pretty(&index)?;
    let mut idx_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&index_path)?;
    idx_file.write_all(serialized_index.as_bytes())?;

    Ok(())
}

pub fn reject_discovered_skill(name: &str) -> Result<()> {
    let home = home_dir_path();
    let brain_dir = home.join(".nir/brain");
    let skills_dir = brain_dir.join("skills");
    let index_path = brain_dir.join("skills_index.json");

    let mut index = if index_path.exists() {
        let content = fs::read_to_string(&index_path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        SkillsIndex::default()
    };

    let slug = to_slug(name);
    index
        .discovered_skills
        .retain(|skill| skill.name != slug);

    let payload_path = skills_dir.join(format!("{}.json", slug));
    if payload_path.exists() {
        fs::remove_file(&payload_path)?;
    }

    let serialized_index = serde_json::to_string_pretty(&index)?;
    let mut idx_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&index_path)?;
    idx_file.write_all(serialized_index.as_bytes())?;

    Ok(())
}

fn to_slug(s: &str) -> String {
    s.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ' && c != '-', "")
        .replace(' ', "-")
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<&str>>()
        .join("-")
}

fn clean_title(category: &str, summary: &str) -> String {
    let raw_title = format!("{}: {}", category, summary.trim());
    let mut clean = raw_title.replace('\n', " ").replace('\r', " ");
    if clean.len() > 64 {
        clean.truncate(61);
        clean.push_str("...");
    }
    clean
}

fn clean_description(category: &str, summary: &str) -> String {
    let raw_desc = format!(
        "Synthesized guidelines for {} tasks based on: {}.",
        category,
        summary.trim()
    );
    let mut clean = raw_desc.replace('\n', " ").replace('\r', " ");
    if clean.len() > 1024 {
        clean.truncate(1021);
        clean.push_str("...");
    }
    clean
}

pub async fn write_promoted_skill(
    category: &str,
    summary: &str,
    instruction_override: &str,
) -> Result<String> {
    let home = home_dir_path();
    let brain_dir = home.join(".nir/brain");
    let index_path = brain_dir.join("skills_index.json");
    let skills_dir = brain_dir.join("skills");
    
    fs::create_dir_all(&skills_dir).context("Failed to create skills directory")?;

    let mut index = if index_path.exists() {
        let content = fs::read_to_string(&index_path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        SkillsIndex::default()
    };

    let clean_name = clean_title(category, summary);
    let clean_desc = clean_description(category, summary);
    let filename = to_slug(&clean_name);

    let is_similar = |existing: &str, new_slug: &str| -> bool {
        if existing == new_slug { return true; }
        let existing_stems: std::collections::HashSet<String> = existing
            .split('-')
            .map(|w| w.chars().take(4).collect::<String>())
            .collect();
        let new_stems: std::collections::HashSet<String> = new_slug
            .split('-')
            .map(|w| w.chars().take(4).collect::<String>())
            .collect();

        let intersection = existing_stems.intersection(&new_stems).count();
        intersection >= 2
    };

    if !index.active_skills.iter().any(|s| is_similar(&s.name, &filename))
        && !index.discovered_skills.iter().any(|s| is_similar(&s.name, &filename))
    {
        index.discovered_skills.push(SkillIndexEntry {
            name: filename.clone(),
            description: clean_desc.clone(),
            last_used_timestamp: chrono::Utc::now().timestamp(),
        });

        let serialized_index = serde_json::to_string_pretty(&index)?;
        let mut idx_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&index_path)?;
        idx_file.write_all(serialized_index.as_bytes())?;
    }

    let payload = SkillPayload {
        name: filename.clone(),
        description: clean_desc,
        system_instruction_override: instruction_override.to_string(),
    };

    let payload_path = skills_dir.join(format!("{}.json", filename));
    let serialized_payload = serde_json::to_string_pretty(&payload)?;
    let mut payload_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&payload_path)?;
    payload_file.write_all(serialized_payload.as_bytes())?;

    Ok(filename)
}

/// Simple category matching heuristic before triggering the LLM
pub fn inject_relevant_payloads(user_message: &str, system_prompt: &mut String) {
    let home = home_dir_path();
    let brain_dir = home.join(".nir/brain");
    let skills_dir = brain_dir.join("skills");
    let index_path = brain_dir.join("skills_index.json");
    
    if !skills_dir.exists() { return; }

    let lowercase_message = user_message.to_lowercase();
    let words: std::collections::HashSet<&str> = lowercase_message
        .split(|c: char| !c.is_alphanumeric())
        .collect();

    let mut triggered_skills = Vec::new();

    // Check if the user message touches an active domain (e.g., "ui", "bug", "refactor")
    if let Ok(entries) = std::fs::read_dir(&skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "json") {
                let filename = path.file_stem().unwrap().to_string_lossy().to_string();
                let category = filename.split('-').next().unwrap_or("");
                
                // If the current context matches the skill category, inject the heavy instructions
                if !category.is_empty() && words.contains(category.to_lowercase().as_str()) {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(payload) = serde_json::from_str::<SkillPayload>(&content) {
                            system_prompt.push_str(&format!(
                                "\n[ACTIVE CAPABILITY OVERRIDE: {}]\n{}\n",
                                payload.name, payload.system_instruction_override
                            ));
                            triggered_skills.push(payload.name);
                        }
                    }
                }
            }
        }
    }

    // Refresh last_used_timestamp for all triggered skills to reset their expiration clock
    if !triggered_skills.is_empty() && index_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&index_path) {
            if let Ok(mut index) = serde_json::from_str::<SkillsIndex>(&content) {
                let mut updated = false;
                let now = chrono::Utc::now().timestamp();
                for skill in &mut index.active_skills {
                    if triggered_skills.contains(&skill.name) {
                        skill.last_used_timestamp = now;
                        updated = true;
                    }
                }
                if updated {
                    if let Ok(serialized) = serde_json::to_string_pretty(&index) {
                        if let Ok(mut file) = OpenOptions::new().write(true).truncate(true).open(&index_path) {
                            let _ = file.write_all(serialized.as_bytes());
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_similarity_overlap() {
        // Set A: { "apple", "banana", "cherry" }
        // Set B: { "apple", "banana" }
        // Overlap similarity: intersection (2) / min_size (2) = 1.0
        let s1 = "apple banana cherry";
        let s2 = "apple banana";
        let score = RecollectionsRegistry::calculate_similarity(s1, s2);
        assert!((score - 1.0).abs() < f32::EPSILON, "Expected 1.0, got {}", score);

        // A and B have different lengths, some overlap
        // Set A: { "apple", "banana", "cherry", "date" } -> size 4
        // Set B: { "apple", "banana", "elderberry" } -> size 3
        // Intersection: { "apple", "banana" } -> size 2
        // Min size: 3
        // Overlap: 2 / 3 = 0.6666...
        let s3 = "apple banana cherry date";
        let s4 = "apple banana elderberry";
        let score_partial = RecollectionsRegistry::calculate_similarity(s3, s4);
        assert!((score_partial - 0.6666667).abs() < 1e-5, "Expected ~0.6667, got {}", score_partial);

        // Empty cases
        assert_eq!(RecollectionsRegistry::calculate_similarity("", "apple"), 0.0);
    }

    #[test]
    fn test_write_promoted_skill_formatting() {
        let title = clean_title("Git", "feature: implemented a robust logging system");
        assert_eq!(title, "Git: feature: implemented a robust logging system");

        let long_summary = "this is an extremely long summary that exceeds sixty four characters to test truncation and suffixing";
        let title_truncated = clean_title("Refactor", long_summary);
        assert!(title_truncated.len() <= 64);
        assert!(title_truncated.ends_with("..."));

        let slug = to_slug("Git: feature: implemented a robust logging system");
        assert_eq!(slug, "git-feature-implemented-a-robust-logging-system");
    }
}
