//! Contains helper functions for constructing URLs to various /nir-related pages.
//!
//! These URLs will adapt to the configured server URL in order to construct
//! links appropriate for the environment (e.g., by linking to a local copy of
//! nir.dev in development).
// TODO: rename file to nir_urls.rs

use gpui::App;
use settings::Settings;

use crate::ClientSettings;

fn server_url(cx: &App) -> &str {
    &ClientSettings::get_global(cx).server_url
}

/// Returns the URL to the account page on nir.dev.
pub fn account_url(cx: &App) -> String {
    format!("{server_url}/account", server_url = server_url(cx))
}

/// Returns the URL to the start trial page on nir.dev.
pub fn start_trial_url(cx: &App) -> String {
    format!(
        "{server_url}/account/start-trial",
        server_url = server_url(cx)
    )
}

/// Returns the URL to the upgrade page on nir.dev.
pub fn upgrade_to_zed_pro_url(cx: &App) -> String {
    format!("{server_url}/account/upgrade", server_url = server_url(cx))
}

/// Returns the URL to /nir's terms of service.
pub fn terms_of_service(cx: &App) -> String {
    format!("{server_url}/terms-of-service", server_url = server_url(cx))
}

/// Returns the URL to /nir AI's privacy and security docs.
pub fn ai_privacy_and_security(cx: &App) -> String {
    format!(
        "{server_url}/docs/ai/privacy-and-security",
        server_url = server_url(cx)
    )
}

/// Returns the URL to /nir's edit prediction documentation.
pub fn edit_prediction_docs(cx: &App) -> String {
    format!(
        "{server_url}/docs/ai/edit-prediction",
        server_url = server_url(cx)
    )
}

pub fn skills_docs(cx: &App) -> String {
    format!("{server_url}/docs/ai/skills", server_url = server_url(cx))
}

pub fn rules_docs(cx: &App) -> String {
    format!("{server_url}/docs/ai/rules", server_url = server_url(cx))
}

/// Returns the URL to /nir's ACP registry blog post.
pub fn acp_registry_blog(cx: &App) -> String {
    format!(
        "{server_url}/blog/acp-registry",
        server_url = server_url(cx)
    )
}


pub fn shared_agent_thread_url(session_id: &str) -> String {
    format!("zed://agent/shared/{}", session_id)
}
