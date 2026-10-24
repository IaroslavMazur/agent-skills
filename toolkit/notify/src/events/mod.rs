//! Portable Claude Code and Codex event handling.
//!
//! Handlers consume JSON values, a small session-state view, and a notification
//! sink. This keeps event parsing testable on every platform and lets the binary
//! choose the SQLite and macOS implementations.

use std::{
    env,
    path::{Component, Path},
};

use serde_json::{Map, Value};

use crate::{
    config::AppConfig,
    error::{AppError, Result},
    filters::{
        should_send_codex_notification, should_send_completion_notification, should_send_failure_notification,
        should_send_permission_notification,
    },
    model::Client,
    notifier::{
        NotificationSink, completion_notification, failure_notification, input_notification, permission_notification,
        question_notification,
    },
};

const CODEX_EVENT_TYPE: &str = "agent-turn-complete";
const DEFAULT_FAILURE_MESSAGE: &str = "Claude Code API error";
const DEFAULT_QUESTION: &str = "Claude is asking a question";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JobInfo {
    pub job_number: Option<i64>,
    pub duration_seconds: Option<i64>,
    pub prompt: Option<String>,
}

/// Minimal state required by native hook handlers.
pub trait SessionState {
    fn track_prompt(&self, session_id: &str, prompt: &str, cwd: &str);
    fn mark_stopped(&self, session_id: &str);
    fn mark_waiting(&self, session_id: &str);
    fn active_prompt(&self, session_id: &str) -> Option<String>;
    fn job_info(&self, session_id: &str) -> JobInfo;
    fn cleanup_if_due(&self);
}

/// Parses a JSON object and applies the common CLI payload validation.
pub fn parse_payload(payload: &str) -> Result<Value> {
    let value = serde_json::from_str(payload)
        .map_err(|error| AppError::hook_payload(format!("Failed to parse JSON: {error}")))?;
    validate_payload(&value)?;
    Ok(value)
}

/// Rejects non-object payloads, traversal-bearing cwd values, and invalid session IDs.
pub fn validate_payload(payload: &Value) -> Result<()> {
    let object = payload.as_object().ok_or_else(|| AppError::hook_payload("JSON payload must be an object"))?;
    if let Some(cwd) = object.get("cwd") {
        let cwd = cwd.as_str().ok_or_else(|| AppError::hook_payload("cwd must be a string"))?;
        if Path::new(cwd).components().any(|component| component == Component::ParentDir) {
            return Err(AppError::hook_payload("Path traversal detected in cwd"));
        }
    }
    if let Some(session_id) = object.get("session_id") {
        let session_id = session_id.as_str().ok_or_else(|| AppError::hook_payload("session_id must be a string"))?;
        if session_id.is_empty() || session_id.len() > 255 {
            return Err(AppError::hook_payload("Invalid session_id"));
        }
    }
    Ok(())
}

/// Records user intent, except for Claude's internal subagent completion envelopes.
pub fn handle_user_prompt(payload: &Value, state: &impl SessionState) -> Result<()> {
    validate_payload(payload)?;
    let object = object(payload)?;
    let session_id = required_session_id(object)?;
    let prompt = string(object, "prompt");
    if !is_internal_agent_notification(prompt) {
        state.track_prompt(session_id, prompt, string(object, "cwd"));
    }
    Ok(())
}

/// Marks a completed turn and emits a filtered completion notification.
pub fn handle_stop(
    payload: &Value,
    state: &impl SessionState,
    config: &AppConfig,
    notifier: &mut impl NotificationSink,
) -> Result<()> {
    validate_payload(payload)?;
    let object = object(payload)?;
    let session_id = required_session_id(object)?;
    if nonempty_collection(object.get("background_tasks")) || nonempty_collection(object.get("session_crons")) {
        return Ok(());
    }
    let prompt = state.active_prompt(session_id);
    state.mark_stopped(session_id);
    let duration = prompt
        .as_ref()
        .and_then(|_| state.job_info(session_id).duration_seconds)
        .and_then(|value| u64::try_from(value).ok());
    if let (Some(duration), Some(prompt)) = (duration, prompt) &&
        should_send_completion_notification(&prompt, duration, config)
    {
        let notification = completion_notification(
            Client::Claude,
            project_name(string(object, "cwd")).as_str(),
            &prompt,
            string(object, "last_assistant_message"),
            Some(&format_duration(duration)),
        );
        let _ = notifier.deliver(&notification);
    }
    state.cleanup_if_due();
    Ok(())
}

/// Records a failed turn and emits an all-mode-only failure notification.
pub fn handle_stop_failure(
    payload: &Value,
    state: &impl SessionState,
    config: &AppConfig,
    notifier: &mut impl NotificationSink,
) -> Result<()> {
    validate_payload(payload)?;
    let object = object(payload)?;
    let session_id = required_session_id(object)?;
    let prompt = state.active_prompt(session_id);
    state.mark_stopped(session_id);
    let job = prompt.as_ref().map(|_| state.job_info(session_id));
    if should_send_failure_notification(config) {
        let duration = job
            .as_ref()
            .and_then(|job| job.duration_seconds)
            .and_then(|value| u64::try_from(value).ok())
            .map(format_duration);
        let notification = failure_notification(
            project_name(string(object, "cwd")).as_str(),
            prompt.as_deref().unwrap_or_default(),
            &failure_message(object),
            duration.as_deref(),
        );
        let _ = notifier.deliver(&notification);
    }
    Ok(())
}

/// Tracks Claude's waiting state and alerts only for actionable notification types.
pub fn handle_notification(
    payload: &Value,
    state: &impl SessionState,
    config: &AppConfig,
    notifier: &mut impl NotificationSink,
) -> Result<()> {
    validate_payload(payload)?;
    let object = object(payload)?;
    let session_id = required_session_id(object)?;
    let message = string(object, "message");
    let notification_type = string(object, "notification_type");
    let fallback = match notification_type {
        "agent_needs_input" => Some("Claude needs your input"),
        "quota_auto_resume_stale" => Some("Quota auto-resume needs attention"),
        "quota_auto_resume_disabled" => Some("Quota auto-resume is disabled"),
        _ => None,
    };
    if fallback.is_some() ||
        notification_type == "idle_prompt" ||
        ["waiting for input", "waiting for user", "approval needed"]
            .iter()
            .any(|keyword| message.to_ascii_lowercase().contains(keyword))
    {
        state.mark_waiting(session_id);
    }
    if let Some(fallback) = fallback &&
        should_send_permission_notification(config)
    {
        let detail = [message, string(object, "title")]
            .into_iter()
            .map(str::trim)
            .find(|value| !value.is_empty())
            .unwrap_or(fallback);
        let notification = input_notification(
            &project_name(string(object, "cwd")),
            &state.active_prompt(session_id).unwrap_or_default(),
            detail,
        );
        let _ = notifier.deliver(&notification);
    }
    Ok(())
}

/// Observes an MCP input request without exposing its URL, schema, or authorization fields.
pub fn handle_elicitation(
    payload: &Value,
    state: &impl SessionState,
    config: &AppConfig,
    notifier: &mut impl NotificationSink,
) -> Result<()> {
    validate_payload(payload)?;
    if !should_send_permission_notification(config) {
        return Ok(());
    }
    let object = object(payload)?;
    let task = optional_session_id(object).and_then(|session_id| state.active_prompt(session_id)).unwrap_or_default();
    let server = string(object, "mcp_server_name").trim();
    let message = string(object, "message").trim();
    let message = if message.is_empty() { "Input requested" } else { message };
    let detail = if server.is_empty() { message.to_owned() } else { format!("{server} — {message}") };
    let notification = input_notification(&project_name(string(object, "cwd")), &task, &detail);
    let _ = notifier.deliver(&notification);
    Ok(())
}

/// Emits a permission-request notification when enabled; sessions are optional for this event.
pub fn handle_permission(
    payload: &Value,
    state: &impl SessionState,
    config: &AppConfig,
    notifier: &mut impl NotificationSink,
) -> Result<()> {
    validate_payload(payload)?;
    if !should_send_permission_notification(config) {
        return Ok(());
    }
    let object = object(payload)?;
    let task = optional_session_id(object).and_then(|session_id| state.active_prompt(session_id)).unwrap_or_default();
    let notification = permission_notification(
        Client::Claude,
        project_name(string(object, "cwd")).as_str(),
        &task,
        &permission_request(object.get("tool_name"), object.get("tool_input")),
    );
    let _ = notifier.deliver(&notification);
    Ok(())
}

/// Emits an AskUserQuestion notification when enabled; sessions are optional for this event.
pub fn handle_ask_user_question(
    payload: &Value,
    state: &impl SessionState,
    config: &AppConfig,
    notifier: &mut impl NotificationSink,
) -> Result<()> {
    validate_payload(payload)?;
    if !should_send_permission_notification(config) {
        return Ok(());
    }
    let object = object(payload)?;
    let task = optional_session_id(object).and_then(|session_id| state.active_prompt(session_id)).unwrap_or_default();
    let notification = question_notification(
        Client::Claude,
        project_name(string(object, "cwd")).as_str(),
        &task,
        &question_text(object.get("tool_input")),
    );
    let _ = notifier.deliver(&notification);
    Ok(())
}

/// Tracks native Codex turns and emits completion or attention notifications.
pub fn handle_codex_hook(
    payload: &Value,
    state: &impl SessionState,
    config: &AppConfig,
    notifier: &mut impl NotificationSink,
) -> Result<()> {
    validate_payload(payload)?;
    let object = object(payload)?;
    let session_id = required_session_id(object)?;
    let turn_id = string(object, "turn_id");
    if turn_id.is_empty() {
        return Err(AppError::hook_payload("Codex hook payload must include turn_id"));
    }
    // Steering can submit multiple prompts per turn; isolate state from other turns and clients.
    let key = format!("codex:{session_id}:{turn_id}");
    match string(object, "hook_event_name") {
        "UserPromptSubmit" => state.track_prompt(&key, string(object, "prompt"), string(object, "cwd")),
        "Stop" => {
            let reply = object.get("last_assistant_message");
            validate_codex_message_text(reply, "last_assistant_message")?;
            // A completed turn must not fall back to an earlier prompt submitted during steering.
            if state.job_info(&key).prompt.is_some() {
                return Ok(());
            }
            let prompt = state.active_prompt(&key);
            if prompt.is_some() {
                state.mark_stopped(&key);
            }
            let prompt = prompt.as_deref().unwrap_or_default();
            if should_send_codex_notification(prompt, config) {
                let notification = completion_notification(
                    Client::Codex,
                    &project_name(string(object, "cwd")),
                    prompt,
                    &extract_message_text(reply),
                    None,
                );
                let _ = notifier.deliver(&notification);
            }
            state.cleanup_if_due();
        }
        "PermissionRequest" if should_send_permission_notification(config) => {
            let notification = permission_notification(
                Client::Codex,
                &project_name(string(object, "cwd")),
                &state.active_prompt(&key).unwrap_or_default(),
                &permission_request(object.get("tool_name"), object.get("tool_input")),
            );
            let _ = notifier.deliver(&notification);
        }
        "PreToolUse"
            if string(object, "tool_name") == "request_user_input" && should_send_permission_notification(config) =>
        {
            if let Some(question) = nonempty_question_text(object.get("tool_input")) {
                let notification = question_notification(
                    Client::Codex,
                    &project_name(string(object, "cwd")),
                    &state.active_prompt(&key).unwrap_or_default(),
                    &question,
                );
                let _ = notifier.deliver(&notification);
            }
        }
        "PermissionRequest" | "PreToolUse" => {}
        "Interrupt" => {
            if state.active_prompt(&key).is_some() {
                state.mark_stopped(&key);
            }
        }
        _ => {
            return Err(AppError::hook_payload(
                "Codex hook must be UserPromptSubmit, Stop, PermissionRequest, PreToolUse, or Interrupt",
            ));
        }
    }
    Ok(())
}

/// Handles Codex's legacy completion callback without reading or mutating session state.
pub fn handle_codex_notify(payload: &Value, config: &AppConfig, notifier: &mut impl NotificationSink) -> Result<()> {
    validate_payload(payload)?;
    let object = object(payload)?;
    let event = first_value(object, &["type", "event"])
        .and_then(Value::as_str)
        .filter(|event| !event.is_empty())
        .ok_or_else(|| AppError::hook_payload("Codex payload must include type \"agent-turn-complete\""))?;
    if event != CODEX_EVENT_TYPE {
        return Ok(());
    }
    let input_messages = first_value(object, &["input-messages", "input_messages", "inputMessages"]);
    validate_codex_input_messages(input_messages)?;
    let last_assistant_message =
        first_value(object, &["last-assistant-message", "last_assistant_message", "lastAssistantMessage"]);
    validate_codex_message_text(last_assistant_message, "last-assistant-message")?;
    let prompt = extract_last_user_message(input_messages);
    if should_send_codex_notification(&prompt, config) {
        let cwd = string(object, "cwd");
        let project = if cwd.is_empty() {
            project_name(
                &env::current_dir().ok().and_then(|path| path.into_os_string().into_string().ok()).unwrap_or_default(),
            )
        } else {
            project_name(cwd)
        };
        let result = extract_message_text(last_assistant_message);
        let notification = completion_notification(Client::Codex, &project, &prompt, &result, None);
        let _ = notifier.deliver(&notification);
    }
    Ok(())
}

pub fn format_duration(seconds: u64) -> String {
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;
    if minutes < 60 {
        return if remaining_seconds == 0 { format!("{minutes}m") } else { format!("{minutes}m{remaining_seconds}s") };
    }
    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;
    if remaining_minutes == 0 { format!("{hours}h") } else { format!("{hours}h{remaining_minutes}m") }
}

pub fn permission_request(tool_name: Option<&Value>, tool_input: Option<&Value>) -> String {
    let mut tool_name = tool_name.and_then(Value::as_str).unwrap_or_default().trim().to_owned();
    let mut detail = "";
    if let Some(input) = tool_input.and_then(Value::as_object) {
        if tool_name.is_empty() {
            tool_name = string(input, "name").trim().to_owned();
        }
        detail = ["command", "file_path", "notebook_path", "path", "url", "query", "description"]
            .into_iter()
            .map(|key| string(input, key).trim())
            .find(|value| !value.is_empty())
            .unwrap_or_default();
    }
    match (tool_name.is_empty(), detail.is_empty()) {
        (false, false) => format!("{tool_name} — {detail}"),
        (false, true) => tool_name,
        (true, false) => detail.to_owned(),
        (true, true) => "Permission requested".to_owned(),
    }
}

pub fn question_text(tool_input: Option<&Value>) -> String {
    nonempty_question_text(tool_input).unwrap_or_else(|| DEFAULT_QUESTION.to_owned())
}

fn nonempty_question_text(tool_input: Option<&Value>) -> Option<String> {
    let questions = tool_input?.as_object()?.get("questions")?.as_array()?;
    let questions = questions
        .iter()
        .filter_map(|question| question.as_object().map(|value| string(value, "question").trim()))
        .filter(|question| !question.is_empty())
        .collect::<Vec<_>>();
    match questions.first() {
        None => None,
        Some(first) if questions.len() == 1 => Some((*first).to_owned()),
        Some(first) => Some(format!("{first} (+{} more)", questions.len() - 1)),
    }
}

pub fn failure_message(object: &Map<String, Value>) -> String {
    ["last_assistant_message", "error_details", "error"]
        .into_iter()
        .map(|key| string(object, key).split_whitespace().collect::<Vec<_>>().join(" "))
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_FAILURE_MESSAGE.to_owned())
}

pub fn extract_last_user_message(messages: Option<&Value>) -> String {
    match messages {
        Some(Value::String(value)) => value.trim().to_owned(),
        Some(Value::Array(messages)) => {
            let mut user = Vec::new();
            let mut other = Vec::new();
            for message in messages {
                let text = extract_message_text(Some(message));
                if text.is_empty() {
                    continue;
                }
                if message.as_object().and_then(|item| item.get("role")).and_then(Value::as_str) == Some("user") {
                    user.push(text);
                } else {
                    other.push(text);
                }
            }
            user.into_iter().last().or_else(|| other.into_iter().last()).unwrap_or_default()
        }
        _ => String::new(),
    }
}

pub fn extract_message_text(message: Option<&Value>) -> String {
    match message {
        Some(Value::String(value)) => value.trim().to_owned(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| extract_message_text(Some(item)))
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        Some(Value::Object(object)) => ["content", "text", "message"]
            .into_iter()
            .find_map(|key| object.get(key))
            .map_or_else(String::new, |value| extract_message_text(Some(value))),
        _ => String::new(),
    }
}

fn validate_codex_input_messages(messages: Option<&Value>) -> Result<()> {
    match messages {
        None => Ok(()),
        Some(messages) if !matches!(messages, Value::Object(_)) && is_codex_message_text(messages) => Ok(()),
        Some(_) => {
            Err(AppError::hook_payload("Codex input-messages must be a string or an array of strings or objects"))
        }
    }
}

fn validate_codex_message_text(message: Option<&Value>, field: &str) -> Result<()> {
    match message {
        None | Some(Value::Null) => Ok(()),
        Some(message) if is_codex_message_text(message) => Ok(()),
        Some(_) => Err(AppError::hook_payload(format!("Codex {field} must be a string, array, or object"))),
    }
}

fn is_codex_message_text(message: &Value) -> bool {
    match message {
        Value::String(_) => true,
        Value::Array(items) => items.iter().all(is_codex_message_text),
        Value::Object(object) => {
            ["content", "text", "message"].into_iter().find_map(|key| object.get(key)).is_none_or(is_codex_message_text)
        }
        _ => false,
    }
}

fn object(payload: &Value) -> Result<&Map<String, Value>> {
    payload.as_object().ok_or_else(|| AppError::hook_payload("JSON payload must be an object"))
}
fn string<'a>(object: &'a Map<String, Value>, key: &str) -> &'a str {
    object.get(key).and_then(Value::as_str).unwrap_or_default()
}
fn required_session_id(object: &Map<String, Value>) -> Result<&str> {
    optional_session_id(object).ok_or_else(|| AppError::hook_payload("Missing session_id in input"))
}
fn optional_session_id(object: &Map<String, Value>) -> Option<&str> {
    object.get("session_id").and_then(Value::as_str).filter(|value| !value.is_empty())
}
fn first_value<'a>(object: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| object.get(*key))
}
fn nonempty_collection(value: Option<&Value>) -> bool {
    value.is_some_and(|value| match value {
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
        Value::Null => false,
        _ => true,
    })
}
fn project_name(cwd: &str) -> String {
    Path::new(cwd).file_name().and_then(|name| name.to_str()).unwrap_or_default().to_owned()
}

fn is_internal_agent_notification(prompt: &str) -> bool {
    let clean = strip_terminal_controls(prompt);
    let trimmed = clean.trim_start().to_ascii_lowercase();
    ["<subagent_notification", "<task-notification"].iter().any(|prefix| {
        trimmed.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('>') || rest.starts_with(char::is_whitespace))
    })
}

fn strip_terminal_controls(value: &str) -> String {
    strip_ansi_escapes::strip_str(value).chars().filter(|character| !character.is_control()).collect()
}

#[cfg(test)]
mod tests;
