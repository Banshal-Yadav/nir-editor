use serde::{Deserialize, Serialize};
use agent_skills::{SkillMetadata, SKILL_FILE_NAME, global_skills_dir};
use anyhow::{Result, Context};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use futures::StreamExt;
use gpui::AsyncApp;
use language_model::{
    LanguageModel, LanguageModelRequest, LanguageModelRequestMessage,
    MessageContent, Role, CompletionIntent, LanguageModelCompletionEvent,
};
use nir_analytics::{AnalyticsState, load_analytics_state, save_analytics_state};

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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnalyticsConfig {
    pub enabled: bool,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl AnalyticsConfig {
    pub fn load() -> Self {
        let path = home_dir_path()
            .join(".nir")
            .join("brain")
            .join("analytics_config.json");
        if !path.exists() {
            return Self::default();
        }
        fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let dir = home_dir_path().join(".nir").join("brain");
        fs::create_dir_all(&dir).context("Failed to create .nir/brain directory")?;
        let path = dir.join("analytics_config.json");
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize analytics config")?;
        fs::write(&path, content).context("Failed to write analytics config")?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ManualAnalysisStats {
    pub total_logs: usize,
    pub parsed_checkpoints: usize,
    pub staged_recollections: usize,
    pub eligible_for_promotion: usize,
    pub discovered_skills: usize,
}

/// Get stats for manual analysis display (no model needed).
pub fn get_manual_analysis_stats() -> ManualAnalysisStats {
    let brain_dir = home_dir_path().join(".nir").join("brain");
    let recollections_path = brain_dir.join("recollections.json");
    let index_path = brain_dir.join("skills_index.json");
    let logs_dir = brain_dir.join("logs");

    // Parse all log lines to count actual checkpoints
    let mut total_logs = 0;
    let mut parsed_checkpoints = 0;
    if logs_dir.exists() {
        if let Ok(entries) = fs::read_dir(&logs_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().map_or(false, |e| e == "md") {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        for line in content.lines() {
                            if line.starts_with('[') {
                                total_logs += 1;
                                if process_log_line(line).is_some() {
                                    parsed_checkpoints += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Load registry
    let registry: RecollectionsRegistry = if recollections_path.exists() {
        fs::read_to_string(&recollections_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default()
    } else {
        RecollectionsRegistry::default()
    };

    let staged_recollections = registry.staged_recollections.len();
    let eligible_for_promotion = registry.check_promotion_targets().len();

    // Count discovered skills
    let discovered_skills = if index_path.exists() {
        fs::read_to_string(&index_path)
            .ok()
            .and_then(|c| serde_json::from_str::<SkillsIndex>(&c).ok())
            .map(|idx| idx.discovered_skills.len())
            .unwrap_or(0)
    } else {
        0
    };

    ManualAnalysisStats {
        total_logs,
        parsed_checkpoints,
        staged_recollections,
        eligible_for_promotion,
        discovered_skills,
    }
}

/// Clean up analytics state: remove recollections registry and reset config.
pub fn cleanup_analytics() -> Result<()> {
    let brain_dir = home_dir_path().join(".nir").join("brain");
    let recollections_path = brain_dir.join("recollections.json");
    
    if recollections_path.exists() {
        fs::remove_file(&recollections_path)
            .context("Failed to remove recollections registry")?;
    }
    
    // Reset config to disabled after cleanup
    let config = AnalyticsConfig { enabled: false };
    config.save()?;
    
    Ok(())
}

// Windows shells can isolate USERPROFILE vs HOME inconsistently across PowerShell/WSL/MSYS;
// strict ordering + `.` fallback prevents telemetry from fragmenting to silent directories.
pub(crate) fn home_dir_path() -> PathBuf {
    if let Ok(path) = std::env::var("USERPROFILE") {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("HOME") {
        return PathBuf::from(path);
    }
    PathBuf::from(".")
}

impl RecollectionsRegistry {
    /// Process a newly ingested checkpoint into the current registry state.
    /// Uses the two-tiered hybrid similarity gate from `nir_analytics`.
    /// `reflections_used` is a shared counter — pass it across calls to cap
    /// total LLM reflective gate calls per analysis cycle (avoids runaway
    /// costs with large log histories on slow local models).
    pub async fn merge_checkpoint(
        &mut self,
        checkpoint: ParsedCheckpoint,
        model: Arc<dyn LanguageModel>,
        cx: &AsyncApp,
        reflections_used: &mut usize,
        max_reflections: usize,
    ) {
        let mut matched_index: Option<usize> = None;

        for (index, staged) in self.staged_recollections.iter().enumerate() {
            if staged.category.to_uppercase() != checkpoint.category.to_uppercase() {
                continue;
            }

            for existing_summary in &staged.associated_summaries {
                match nir_analytics::evaluate_match(&checkpoint.summary, existing_summary) {
                    nir_analytics::MatchResult::DirectMerge(_) => {
                        matched_index = Some(index);
                        break;
                    }
                    nir_analytics::MatchResult::RequiresReflection(_) => {
                        if *reflections_used >= max_reflections {
                            log::debug!("Skill Discovery: reflective gate cap ({}) reached, skipping LLM call", max_reflections);
                            continue;
                        }
                        *reflections_used += 1;
                        let is_match =
                            run_reflective_gate(&checkpoint.summary, existing_summary, model.clone(), cx)
                                .await
                                .unwrap_or(false);
                        if is_match {
                            matched_index = Some(index);
                            break;
                        }
                    }
                    nir_analytics::MatchResult::NoMatch => continue,
                }
            }

            if matched_index.is_some() {
                break;
            }
        }

        if let Some(index) = matched_index {
            let entry = &mut self.staged_recollections[index];
            if !entry.associated_summaries.contains(&checkpoint.summary) {
                entry.associated_summaries.push(checkpoint.summary.clone());
            }
            if checkpoint.tags.contains(&"error_recovery".to_string()) {
                entry.metrics.had_error_recovery = true;
            }
            if checkpoint.tags.contains(&"user_intervention".to_string()) {
                entry.metrics.user_corrections_count += 1;
            }
            return;
        }

        let slug = checkpoint
            .summary
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
                user_corrections_count: if checkpoint.tags.contains(&"user_intervention".to_string()) {
                    1
                } else {
                    0
                },
            },
            associated_summaries: vec![checkpoint.summary],
            status: "STAGED".to_string(),
        };
        self.staged_recollections.push(new_staged);
    }

    /// Surfaces staged clusters as discover-skill candidates.
    /// Promotion criteria (any one sufficient):
    ///   - 2+ associated summaries (seen the pattern at least twice)
    ///   - error-recovery with 1+ user correction (high-friction, worth capturing immediately)
    pub fn check_promotion_targets(&self) -> Vec<StagedRecollection> {
        self.staged_recollections
            .iter()
            .filter(|staged| {
                if staged.status != "STAGED" {
                    return false;
                }

                let has_enough_summaries = staged.associated_summaries.len() >= 2;
                let high_friction_met = staged.metrics.had_error_recovery
                    && staged.metrics.user_corrections_count >= 1;

                has_enough_summaries || high_friction_met
            })
            .cloned()
            .collect()
    }
}

// =============================================================================
// Two-tiered hybrid gate: reflective layer
// =============================================================================

/// Asynchronous reflective gate for `MatchResult::RequiresReflection`.
/// Asks the LLM whether two tasks are semantically equivalent.
pub async fn run_reflective_gate(
    new_task: &str,
    existing_cluster_summary: &str,
    model: Arc<dyn LanguageModel>,
    cx: &AsyncApp,
) -> Result<bool> {
    let prompt = build_reflection_prompt(new_task, existing_cluster_summary);

    let request = LanguageModelRequest {
        intent: Some(CompletionIntent::UserPrompt),
        messages: vec![LanguageModelRequestMessage {
            role: Role::User,
            content: vec![MessageContent::Text(prompt)],
            cache: false,
            reasoning_details: None,
        }],
        ..Default::default()
    };

    let mut stream = model
        .stream_completion(request, cx)
        .await
        .context("Failed to call language model for reflective gate")?;

    let mut response_text = String::new();
    while let Some(event) = stream.next().await {
        match event.context("Stream error from reflective gate LLM call")? {
            LanguageModelCompletionEvent::Text(text) => response_text.push_str(&text),
            _ => continue,
        }
    }

    let trimmed = response_text.trim();
    let start = trimmed.find('{');
    let end = trimmed.rfind('}');
    let clean_json = match (start, end) {
        (Some(s), Some(e)) if e >= s => &trimmed[s..=e],
        _ => {
            log::warn!("Reflective gate response contained no JSON object: {:?}", trimmed);
            return Ok(false);
        }
    };

    #[derive(Deserialize)]
    struct ReflectionResponse {
        is_semantic_match: bool,
    }

    match serde_json::from_str::<ReflectionResponse>(clean_json) {
        Ok(parsed) => Ok(parsed.is_semantic_match),
        Err(error) => {
            log::warn!("Failed to parse reflective gate JSON: {}", error);
            Ok(false)
        }
    }
}

/// Closure-based variant of `run_reflective_gate` for non-gpui contexts.
pub async fn run_reflective_gate_with_client<C>(
    new_task: &str,
    existing_cluster_summary: &str,
    model_client: C,
) -> Result<bool>
where
    C: FnOnce(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>>,
{
    let prompt = build_reflection_prompt(new_task, existing_cluster_summary);
    let raw_response = model_client(prompt)
        .await
        .context("Model client call failed for reflective gate")?;

    let trimmed = raw_response.trim();
    let start = trimmed.find('{');
    let end = trimmed.rfind('}');
    let clean_json = match (start, end) {
        (Some(s), Some(e)) if e >= s => &trimmed[s..=e],
        _ => {
            log::warn!("Reflective gate response contained no JSON object: {:?}", trimmed);
            return Ok(false);
        }
    };

    #[derive(Deserialize)]
    struct ReflectionResponse {
        is_semantic_match: bool,
    }

    match serde_json::from_str::<ReflectionResponse>(clean_json) {
        Ok(parsed) => Ok(parsed.is_semantic_match),
        Err(error) => {
            log::warn!("Failed to parse reflective gate JSON: {}", error);
            Ok(false)
        }
    }
}

fn build_reflection_prompt(new_task: &str, existing_cluster_summary: &str) -> String {
    format!(
        "You are a semantic equivalence classifier for a skill clustering system.\n\
         \n\
         EXISTING CLUSTER SUMMARY: \"{existing}\"\n\
         NEW TASK: \"{new}\"\n\
         \n\
         Decide if the NEW TASK is semantically equivalent to (or a sub-task of)\n\
         the EXISTING CLUSTER SUMMARY. Reply with a strict JSON object containing\n\
         exactly one field:\n\
         \n\
         {{ \"is_semantic_match\": true }}    -- if they describe the same workflow\n\
         {{ \"is_semantic_match\": false }}   -- otherwise\n\
         \n\
         CRITICAL: Output raw JSON only. No markdown fences, no prose, no commentary.",
        existing = existing_cluster_summary,
        new = new_task,
    )
}

/// Synthesizes raw summaries into structured skill instructions via LLM.
/// Falls back to raw summary dump if the LLM call fails.
pub async fn synthesize_skill_content<C>(
    category: &str,
    summaries: &[String],
    model_client: C,
) -> String
where
    C: FnOnce(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>>,
{
    let summaries_text = summaries
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{}. {}", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "Synthesize these task completion summaries into a reusable skill instruction.\n\
         \n\
         CATEGORY: {category}\n\
         \n\
         SUMMARIES:\n{summaries}\n\
         \n\
         Output a concise skill body with these sections:\n\
         \n\
         ## When to use\n\
         One line describing when this skill activates.\n\
         \n\
         ## Key patterns\n\
         Bullet list of the 2-4 most important patterns across these summaries.\n\
         \n\
         ## Procedure\n\
         Numbered steps the model should follow when this skill fires.\n\
         \n\
         ## Pitfalls\n\
         1-2 things to watch for based on what these summaries reveal.\n\
         \n\
         RULES:\n\
         - Be concise. Max 300 words total.\n\
         - No filler, no preamble, no hedging.\n\
         - Each section must be present but can be 1-2 lines.\n\
         - Output raw markdown only. No code fences around the whole thing.",
        category = category,
        summaries = summaries_text,
    );

    let raw_response = match model_client(prompt).await {
        Ok(r) => r,
        Err(err) => {
            log::warn!("Skill synthesis LLM call failed, using raw summaries: {:?}", err);
            return build_fallback_body(category, summaries);
        }
    };

    let trimmed = raw_response.trim();
    if trimmed.len() < 50 || !trimmed.contains("##") {
        log::warn!("Skill synthesis response too short or malformed, using raw summaries");
        return build_fallback_body(category, summaries);
    }

    trimmed.to_string()
}

fn build_fallback_body(category: &str, summaries: &[String]) -> String {
    let mut body = format!(
        "When working within the '{}' domain, follow these patterns:\n\n",
        category
    );
    for summary in summaries {
        body.push_str(&format!("- {}\n", summary));
    }
    body
}

/// Runs the per-line two-tiered hybrid gate over a batch of log lines.
/// Takes a `model_client` closure for non-gpui contexts.
/// `model_client` must be `Clone` — reflection evaluates against multiple clusters.
pub async fn process_log_lines_with_gate<C>(
    registry: &mut RecollectionsRegistry,
    log_lines: &[String],
    model_client: C,
) -> Result<GateRunStats>
where
    C: Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>>
        + Send
        + Sync
        + Clone
        + 'static,
{
    let mut stats = GateRunStats::default();

    for line in log_lines {
        let Some(checkpoint) = process_log_line(line) else {
            stats.skipped += 1;
            continue;
        };
        stats.parsed += 1;

        let category_upper = checkpoint.category.to_uppercase();
        let mut matched_index: Option<usize> = None;

        for (index, staged) in registry.staged_recollections.iter().enumerate() {
            if staged.category.to_uppercase() != category_upper {
                continue;
            }

            for existing_summary in &staged.associated_summaries {
                let jaccard = nir_analytics::overlap_coefficient(
                    &checkpoint.summary,
                    existing_summary,
                );
                match nir_analytics::evaluate_match(&checkpoint.summary, existing_summary) {
                    nir_analytics::MatchResult::DirectMerge(_) => {
                        log::debug!(
                            "Nir Gate: jaccard={:.3} >= 0.40 → DirectMerge into cluster [{}] (existing summary: \"{}\")",
                            jaccard,
                            index,
                            truncate_for_log(existing_summary, 60)
                        );
                        matched_index = Some(index);
                        break;
                    }
                    nir_analytics::MatchResult::RequiresReflection(_) => {
                        log::debug!(
                            "Nir Gate: jaccard={:.3} in gray zone [0.10, 0.40) → consulting LLM reflection (existing summary: \"{}\")",
                            jaccard,
                            truncate_for_log(existing_summary, 60)
                        );
                        let client = model_client.clone();
                        let is_match = run_reflective_gate_with_client(
                            &checkpoint.summary,
                            existing_summary,
                            client,
                        )
                        .await
                        .unwrap_or(false);
                        stats.reflections += 1;
                        if is_match {
                            stats.reflection_matches += 1;
                            log::debug!(
                                "Nir Gate: LLM reflection returned match=true → merging into cluster [{}]",
                                index
                            );
                            matched_index = Some(index);
                            break;
                        } else {
                            log::debug!("Nir Gate: LLM reflection returned match=false → continuing scan");
                        }
                    }
                    nir_analytics::MatchResult::NoMatch => {
                        log::debug!(
                            "Nir Gate: jaccard={:.3} < 0.10 → NoMatch (existing summary: \"{}\")",
                            jaccard,
                            truncate_for_log(existing_summary, 60)
                        );
                        continue;
                    }
                }
            }

            if matched_index.is_some() {
                break;
            }
        }

        if let Some(index) = matched_index {
            let entry = &mut registry.staged_recollections[index];
            if !entry.associated_summaries.contains(&checkpoint.summary) {
                entry.associated_summaries.push(checkpoint.summary.clone());
            }
            if checkpoint.tags.contains(&"error_recovery".to_string()) {
                entry.metrics.had_error_recovery = true;
            }
            if checkpoint.tags.contains(&"user_intervention".to_string()) {
                entry.metrics.user_corrections_count += 1;
            }
            stats.merged += 1;
        } else {
            let slug = checkpoint
                .summary
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
                    user_corrections_count: if checkpoint
                        .tags
                        .contains(&"user_intervention".to_string())
                    {
                        1
                    } else {
                        0
                    },
                },
                associated_summaries: vec![checkpoint.summary],
                status: "STAGED".to_string(),
            };
            registry.staged_recollections.push(new_staged);
            stats.created += 1;
        }
    }

    Ok(stats)
}

/// Counters returned by `process_log_lines_with_gate` so the worker can log
/// visibility into how many reflections actually fired.
#[derive(Debug, Default, Clone, Copy)]
pub struct GateRunStats {
    pub parsed: usize,
    pub skipped: usize,
    pub merged: usize,
    pub created: usize,
    pub reflections: usize,
    pub reflection_matches: usize,
}

fn truncate_for_log(text: &str, max_chars: usize) -> String {
    let single_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() <= max_chars {
        single_line
    } else {
        let truncated: String = single_line.chars().take(max_chars).collect();
        format!("{}…", truncated)
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

    let category = extract_category(&summary);

    Some(ParsedCheckpoint {
        category,
        summary,
        tags: Vec::new(),
    })
}

/// Keyword-based category extraction to scope cluster comparisons.
fn extract_category(summary: &str) -> String {
    let s = summary.to_lowercase();

    // Philosophy & mindfulness
    if s.contains("philosophy") || s.contains("stoic") || s.contains("nietzsch")
        || s.contains("epicurean") || s.contains("existential") || s.contains("buddhist")
        || s.contains("meditation") || s.contains("mindfulness") || s.contains("consciousness")
        || s.contains("gratitude") || s.contains("resilience") || s.contains("meaning")
        || s.contains("marcus aurelius") || s.contains("amor fati") || s.contains("absurd")
        || s.contains("impermanence") || s.contains("suffering") || s.contains("virtue")
        || s.contains("epistemology") || s.contains("metaphysics") || s.contains("ethics")
        || s.contains("determinism") || s.contains("nihilis") || s.contains("socrates")
        || s.contains("plato") || s.contains("aristotle") || s.contains("kant")
        || s.contains("spinoza") || s.contains("seneca") || s.contains("epictetus")
        || s.contains("philosophical") || s.contains("zen") || s.contains("vipassana")
        || s.contains("dharma") || s.contains("taoism") || s.contains("introspection")
        || s.contains("reflection") || s.contains("wisdom") || s.contains("happiness")
        || s.contains("camus") || s.contains("sartre") || s.contains("kierkegaard")
        || s.contains("schopenhauer") || s.contains("existentialism") || s.contains("absurdism")
        || s.contains("nihilism") || s.contains("stoicism") || s.contains("spirituality")
    {
        return "philosophy".to_string();
    }

    // Software development & coding
    if s.contains("rust") || s.contains("cargo") || s.contains("compile")
        || s.contains("refactor") || s.contains("debug") || s.contains("implement")
        || s.contains("crate") || s.contains("function") || s.contains("struct")
        || s.contains("trait") || s.contains("async") || s.contains("lifetime")
        || s.contains("build error") || s.contains("unit test") || s.contains("api")
        || s.contains("code") || s.contains("coding") || s.contains("programming")
        || s.contains("git") || s.contains("commit") || s.contains("pull request")
        || s.contains("merge") || s.contains("github") || s.contains("repository")
        || s.contains("repo") || s.contains("dependency") || s.contains("json")
        || s.contains("yaml") || s.contains("script") || s.contains("python")
        || s.contains("javascript") || s.contains("typescript") || s.contains("html")
        || s.contains("css") || s.contains("c++") || s.contains("golang")
        || s.contains("sql") || s.contains("database") || s.contains("docker")
        || s.contains("kubernetes") || s.contains("frontend") || s.contains("backend")
        || s.contains("bug") || s.contains("feature") || s.contains("test")
        || s.contains("java") || s.contains("csharp") || s.contains("c#")
        || s.contains("ruby") || s.contains("swift") || s.contains("kotlin")
        || s.contains("haskell") || s.contains("scala") || s.contains("golang")
        || s.contains("php") || s.contains("elixir") || s.contains("bash")
        || s.contains("powershell") || s.contains("sh ") || s.contains("zsh")
        || s.contains("postgres") || s.contains("mysql") || s.contains("sqlite")
        || s.contains("redis") || s.contains("mongodb") || s.contains("graphql")
        || s.contains("grpc") || s.contains("webhook") || s.contains("websocket")
        || s.contains("debugging") || s.contains("refactored") || s.contains("implemented")
        || s.contains("compiler") || s.contains("ast") || s.contains("parser")
        || s.contains("regex") || s.contains("formatter") || s.contains("linter")
        || s.contains("dependency") || s.contains("package") || s.contains("framework")
        || s.contains("library") || s.contains("endpoint") || s.contains("server")
        || s.contains("client") || s.contains("ci/cd") || s.contains("pipeline")
        || s.contains("concurrency") || s.contains("asynchronous") || s.contains("thread")
        || s.contains("testing") || s.contains("integration test") || s.contains("regression")
    {
        return "coding".to_string();
    }

    // Academics, studying & learning
    if s.contains("study") || s.contains("studying") || s.contains("learn")
        || s.contains("learning") || s.contains("read ") || s.contains("reading")
        || s.contains("exam ") || s.contains("exams") || s.contains("syllabus")
        || s.contains("semester") || s.contains("lecture") || s.contains("course")
        || s.contains("subject") || s.contains("academic") || s.contains("university")
        || s.contains("college") || s.contains("school") || s.contains("homework")
        || s.contains("assignment") || s.contains("revision") || s.contains("revise")
        || s.contains("tutorial") || s.contains("math") || s.contains("science")
        || s.contains("physics") || s.contains("chemistry") || s.contains("biology")
        || s.contains("cse") || s.contains("computer science") || s.contains("curriculum")
        || s.contains("preparation") || s.contains("prepare") || s.contains("textbook")
        || s.contains("education") || s.contains("tutorial") || s.contains("concept")
    {
        return "study".to_string();
    }

    // Nir / agent tooling
    if s.contains("skill") || s.contains("analytics") || s.contains("nir ide")
        || s.contains("nir") || s.contains("agent") || s.contains("tool")
        || s.contains("checkpoint") || s.contains("recollection") || s.contains("subagent")
        || s.contains("prompt") || s.contains("llm") || s.contains("assistant")
        || s.contains("copilot") || s.contains("telemetry") || s.contains("ide")
        || s.contains("editor") || s.contains("zed") || s.contains("workspace")
        || s.contains("mcp") || s.contains("agentic") || s.contains("rag")
    {
        return "dev-tools".to_string();
    }

    // Health & fitness
    if s.contains("health") || s.contains("exercise") || s.contains("fitness")
        || s.contains("nutrition") || s.contains("sleep") || s.contains("workout")
        || s.contains("gym") || s.contains("diet") || s.contains("run")
        || s.contains("meds") || s.contains("medicine") || s.contains("doctor")
        || s.contains("therapy") || s.contains("anxiety") || s.contains("depression")
        || s.contains("mental") || s.contains("symptom") || s.contains("yoga")
        || s.contains("sagnik") || s.contains("pantocid") || s.contains("nexito")
        || s.contains("lonazep") || s.contains("oleanz") || s.contains("psychiatrist")
        || s.contains("cardio") || s.contains("calm") || s.contains("stress")
    {
        return "health".to_string();
    }

    // Writing & communication
    if s.contains("wrote") || s.contains("writing") || s.contains("document")
        || s.contains("email") || s.contains("report") || s.contains("article")
        || s.contains("essay") || s.contains("draft") || s.contains("blog")
        || s.contains("readme") || s.contains("docs") || s.contains("documentation")
        || s.contains("slack") || s.contains("discord") || s.contains("meeting")
        || s.contains("presentation") || s.contains("feedback") || s.contains("journal")
        || s.contains("notes") || s.contains("diary") || s.contains("summary")
        || s.contains("summarize") || s.contains("comment") || s.contains("chat")
    {
        return "writing".to_string();
    }

    "general".to_string()
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

pub async fn run_analytics_cycle(
    model: Arc<dyn LanguageModel>,
    cx: &AsyncApp,
) -> Result<Vec<String>> {
    let home = home_dir_path();
    let brain_dir = home.join(".nir/brain");
    let recollections_path = brain_dir.join("recollections.json");
    let logs_dir = brain_dir.join("logs");
    let skills_dir = brain_dir.join("skills");
    let index_path = brain_dir.join("skills_index.json");

    // Load existing registry.
    let mut registry: RecollectionsRegistry = if recollections_path.exists() {
        let content = fs::read_to_string(&recollections_path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        RecollectionsRegistry::default()
    };

    // Load watermark state to only process new log lines.
    let mut watermark_state: AnalyticsState = load_analytics_state().unwrap_or_default();

    // Cap LLM reflection calls per cycle to prevent runaway costs on local models.
    const MAX_REFLECTIONS_PER_CYCLE: usize = 30;
    let mut reflections_used: usize = 0;

    if logs_dir.exists() {
        let mut log_files: Vec<_> = fs::read_dir(&logs_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
            .collect();
        // Sort chronologically.
        log_files.sort_by_key(|e| e.file_name());

        for entry in log_files {
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();

            // Count how many log lines we've already processed for this file.
            let already_processed = watermark_state
                .processed_files
                .get(filename_str.as_ref())
                .copied()
                .unwrap_or(0);

            let log_data = fs::read_to_string(entry.path())?;

            // Collect only the valid log lines (lines starting with '[').
            let log_lines: Vec<&str> = log_data
                .lines()
                .filter(|l| l.trim_start().starts_with('['))
                .collect();

            let new_lines = log_lines.get(already_processed..).unwrap_or(&[]);

            if new_lines.is_empty() {
                log::debug!("Skill Discovery: {} fully processed (watermark={}), skipping", filename_str, already_processed);
                continue;
            }

            log::info!(
                "Skill Discovery: {} — processing {} new line(s) (watermark={})",
                filename_str,
                new_lines.len(),
                already_processed
            );

            for line in new_lines {
                if let Some(checkpoint) = process_log_line(line) {
                    registry.merge_checkpoint(checkpoint, model.clone(), cx, &mut reflections_used, MAX_REFLECTIONS_PER_CYCLE).await;
                }
            }

            // Advance the watermark for this file.
            watermark_state
                .processed_files
                .insert(filename_str.into_owned(), log_lines.len());
        }
    }

    let eligible_targets = registry.check_promotion_targets();
    let mut promoted_skill_names = Vec::new();

    if !eligible_targets.is_empty() {
        let (prompt_tx, mut prompt_rx) = futures::channel::mpsc::unbounded::<(String, futures::channel::oneshot::Sender<Result<String>>)>();
        let model_clone = model.clone();
        let cx_clone = cx.clone();

        cx_clone.spawn(async move |cx| {
            while let Some((prompt, response_tx)) = prompt_rx.next().await {
                let request = LanguageModelRequest {
                    intent: Some(CompletionIntent::UserPrompt),
                    messages: vec![LanguageModelRequestMessage {
                        role: Role::User,
                        content: vec![MessageContent::Text(prompt)],
                        cache: false,
                        reasoning_details: None,
                    }],
                    ..Default::default()
                };

                let res = async {
                    let mut stream = model_clone
                        .stream_completion(request, cx)
                        .await
                        .context("Failed to call language model for skill synthesis")?;

                    let mut response_text = String::new();
                    while let Some(event) = stream.next().await {
                        match event.context("Stream error from skill synthesis LLM call")? {
                            LanguageModelCompletionEvent::Text(text) => response_text.push_str(&text),
                            _ => continue,
                        }
                    }
                    Ok(response_text)
                }.await;

                let _ = response_tx.send(res);
            }
        }).detach();

        for target in eligible_targets {
            let prompt_tx = prompt_tx.clone();
            let instruction_override = synthesize_skill_content(
                &target.category,
                &target.associated_summaries,
                move |prompt| {
                    let prompt_tx = prompt_tx.clone();
                    Box::pin(async move {
                        let (tx, rx) = futures::channel::oneshot::channel::<Result<String>>();
                        prompt_tx.unbounded_send((prompt, tx))
                            .map_err(|_| anyhow::anyhow!("Analytics spawn channel closed"))?;
                        rx.await.map_err(|_| anyhow::anyhow!("Analytics response channel dropped"))?
                    })
                },
            )
            .await;

            match write_promoted_skill(
                &target.category,
                &target.associated_summaries[0],
                &instruction_override,
            ).await {
                Ok(clean_name) => {
                    // Only mark PROMOTED after the file is confirmed written.
                    if let Some(item) = registry.staged_recollections.iter_mut().find(|s| s.id == target.id) {
                        item.status = "PROMOTED".to_string();
                    }
                    promoted_skill_names.push(clean_name);
                }
                Err(err) => {
                    // Log but don't abort — other clusters should still be promoted.
                    log::error!("Skill Discovery: failed to write skill for cluster '{}': {:?}", target.id, err);
                }
            }
        }
    }

    // Expire stale active skills (unused for >30 days).
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
            .create(true).write(true).truncate(true)
            .open(&index_path)?;
        idx_file.write_all(serialized_index.as_bytes())?;
    }

    // Persist the updated registry.
    let serialized = serde_json::to_string_pretty(&registry)?;
    let mut file = OpenOptions::new()
        .create(true).write(true).truncate(true)
        .open(&recollections_path)?;
    file.write_all(serialized.as_bytes())?;

    // Persist the updated watermarks so the next run is incremental.
    if let Err(err) = save_analytics_state(&watermark_state) {
        log::warn!("Skill Discovery: failed to save watermark state: {:?}", err);
    }

    Ok(promoted_skill_names)
}

pub fn approve_discovered_skill(name: &str) -> Result<()> {
    let home = home_dir_path();
    let brain_dir = home.join(".nir/brain");
    let index_path = brain_dir.join("skills_index.json");
    let skills_dir = brain_dir.join("skills");

    let slug = to_slug(name);

    // Read the JSON payload BEFORE touching the index so a missing file
    // doesn't silently nuke the discovered entry.
    let payload_path = skills_dir.join(format!("{}.json", slug));
    let payload_content = fs::read_to_string(&payload_path)
        .with_context(|| format!("Skill payload missing at {}", payload_path.display()))?;
    let payload: SkillPayload = serde_json::from_str(&payload_content)
        .with_context(|| format!("Failed to parse skill payload at {}", payload_path.display()))?;

    let mut index = if index_path.exists() {
        let content = fs::read_to_string(&index_path)?;
        serde_json::from_str(&content).context("Failed to parse skills_index.json")?
    } else {
        SkillsIndex::default()
    };

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

    let active_skill_dir = global_skills_dir().join(&slug);
    fs::create_dir_all(&active_skill_dir)?;

    // Build SKILL.md via serde_yaml_ng — descriptions with `: ` or `..` need quoting.
    let metadata = SkillMetadata {
        name: slug.clone(),
        description: payload.description.clone(),
        disable_model_invocation: false,
    };
    let frontmatter = serde_yaml_ng::to_string(&metadata)
        .context("failed to serialize skill frontmatter as YAML")?;
    let skill_content = format!(
        "---\n{frontmatter}---\n{}",
        payload.system_instruction_override.trim()
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
        serde_json::from_str(&content).context("Failed to parse skills_index.json")?
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

fn clean_title(_category: &str, summary: &str) -> String {
    // Build a slug from the first 6 non-filler words of the summary.
    let filler: std::collections::HashSet<&str> = [
        "a", "an", "the", "on", "in", "of", "to", "and", "for", "how",
        "with", "that", "this", "was", "were", "is", "are", "as",
        "reflected", "explored", "analyzed", "examined", "discussed",
        "practiced", "applied", "reviewed", "used",
    ].iter().copied().collect();

    let words: Vec<&str> = summary
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !filler.contains(&w.to_lowercase().as_str()))
        .take(6)
        .collect();

    if words.is_empty() {
        // Fallback to first 6 raw words.
        summary
            .split_whitespace()
            .take(6)
            .collect::<Vec<_>>()
            .join("-")
    } else {
        words.join("-").to_lowercase()
    }
}

fn clean_description(_category: &str, summary: &str) -> String {
    let mut clean = summary.trim().replace('\n', " ").replace('\r', " ");
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
        let content = fs::read_to_string(&index_path)
            .with_context(|| format!("Failed to read skills index at {}", index_path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse skills index at {}", index_path.display()))?
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

        // 3+ stems so same-category candidates (e.g. "task-completion-*") don't
        // falsely dedupe on the shared "task" + "comp" prefix alone.
        let intersection = existing_stems.intersection(&new_stems).count();
        intersection >= 3
    };

    if !index.active_skills.iter().any(|s| is_similar(&s.name, &filename))
        && !index.discovered_skills.iter().any(|s| is_similar(&s.name, &filename))
    {
        log::info!("Skill Discovery: promoting cluster to skill '{}'", filename);

        index.discovered_skills.push(SkillIndexEntry {
            name: filename.clone(),
            description: clean_desc.clone(),
            last_used_timestamp: chrono::Utc::now().timestamp(),
        });

        // Atomic write: serialize to .tmp then rename. Prevents the
        // "truncate succeeded, write failed, file is 0 bytes" failure mode.
        let serialized_index = serde_json::to_string_pretty(&index)
            .context("Failed to serialize skills index")?;
        let index_tmp = index_path.with_extension("json.tmp");
        fs::write(&index_tmp, serialized_index.as_bytes())
            .with_context(|| format!("Failed to write index tmp at {}", index_tmp.display()))?;
        fs::rename(&index_tmp, &index_path)
            .with_context(|| format!("Failed to rename {} -> {}", index_tmp.display(), index_path.display()))?;
        log::info!("Skill Discovery: wrote skills index to {}", index_path.display());

        let payload = SkillPayload {
            name: filename.clone(),
            description: clean_desc,
            system_instruction_override: instruction_override.to_string(),
        };

        let payload_path = skills_dir.join(format!("{}.json", filename));
        let serialized_payload = serde_json::to_string_pretty(&payload)
            .context("Failed to serialize skill payload")?;
        let payload_tmp = payload_path.with_extension("json.tmp");
        fs::write(&payload_tmp, serialized_payload.as_bytes())
            .with_context(|| format!("Failed to write payload tmp at {}", payload_tmp.display()))?;
        fs::rename(&payload_tmp, &payload_path)
            .with_context(|| format!("Failed to rename {} -> {}", payload_tmp.display(), payload_path.display()))?;
        log::info!(
            "Skill Discovery: wrote skill payload to {}",
            payload_path.display()
        );
    } else {
        log::info!(
            "Skill Discovery: skipping promotion of '{}' — similar to existing skill",
            filename
        );
    }

    Ok(filename)
}

/// Matches active skills against the user message using full slug word overlap.
/// Fires when 2+ tokens (>= 3 chars) from the slug appear in the message.
/// Also refreshes `last_used_timestamp` for triggered skills.
pub fn inject_relevant_payloads(user_message: &str, system_prompt: &mut String) {
    let home = home_dir_path();
    let brain_dir = home.join(".nir/brain");
    let skills_dir = brain_dir.join("skills");
    let index_path = brain_dir.join("skills_index.json");
    
    if !skills_dir.exists() { return; }

    let lowercase_message = user_message.to_lowercase();
    let words: std::collections::HashSet<&str> = lowercase_message
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .collect();

    let mut triggered_skills = Vec::new();

    // Tokenize each slug the same way Jaccard does: split on '-', keep >= 3 chars.
    // Match when 2+ slug tokens appear in the user's message.
    if let Ok(entries) = std::fs::read_dir(&skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "json") {
                let filename = path.file_stem().unwrap().to_string_lossy().to_string();
                let slug_tokens: std::collections::HashSet<&str> = filename
                    .split('-')
                    .filter(|t| t.len() > 2)
                    .collect();

                let overlap = slug_tokens.iter().filter(|t| words.contains(*t)).count();
                if overlap >= 2 {
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

    // Refresh last_used_timestamp for triggered skills
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
                            if let Err(err) = file.write_all(serialized.as_bytes()) {
                                log::error!("Failed to refresh skill timestamps: {:?}", err);
                            }
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
    fn test_overlap_coefficient() {
        let s1 = "apple banana cherry";
        let s2 = "apple banana";
        let score = nir_analytics::overlap_coefficient(s1, s2);
        assert!((score - 1.0).abs() < f32::EPSILON, "Expected 1.0, got {}", score);

        let s3 = "apple banana cherry date";
        let s4 = "apple banana elderberry";
        let score_partial = nir_analytics::overlap_coefficient(s3, s4);
        assert!((score_partial - 0.6666667).abs() < 1e-5, "Expected ~0.6667, got {}", score_partial);

        assert_eq!(nir_analytics::overlap_coefficient("", "apple"), 0.0);
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
