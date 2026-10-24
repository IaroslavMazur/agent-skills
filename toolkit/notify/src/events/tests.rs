use std::cell::RefCell;

use serde_json::json;

use crate::{config::NotificationConfig, model::Notification};

use super::*;

#[derive(Default)]
struct State {
    prompts: RefCell<Vec<String>>,
    stopped: RefCell<Vec<String>>,
    waiting: RefCell<Vec<String>>,
    active: Option<String>,
    job: JobInfo,
}
impl SessionState for State {
    fn track_prompt(&self, session: &str, prompt: &str, _: &str) {
        self.prompts.borrow_mut().push(format!("{session}:{prompt}"));
    }
    fn mark_stopped(&self, session: &str) {
        self.stopped.borrow_mut().push(session.into());
    }
    fn mark_waiting(&self, session: &str) {
        self.waiting.borrow_mut().push(session.into());
    }
    fn active_prompt(&self, _: &str) -> Option<String> {
        self.active.clone()
    }
    fn job_info(&self, _: &str) -> JobInfo {
        self.job.clone()
    }
    fn cleanup_if_due(&self) {}
}
#[derive(Default)]
struct Sink(RefCell<Vec<Notification>>);
impl NotificationSink for Sink {
    fn deliver(&mut self, notification: &Notification) -> bool {
        self.0.borrow_mut().push(notification.clone());
        true
    }
}
fn config() -> AppConfig {
    AppConfig { notification: NotificationConfig { threshold_seconds: 10, ..Default::default() }, ..Default::default() }
}

#[test]
fn validation_rejects_invalid_generic_payload_fields() {
    assert!(validate_payload(&json!({"cwd":"/tmp/../etc"})).is_err());
    assert!(validate_payload(&json!({"cwd":"/tmp/project..next"})).is_ok());
    assert!(validate_payload(&json!({"session_id":""})).is_err());
    assert!(parse_payload("[]").is_err());
}
#[test]
fn user_prompt_tracks_intent_but_ignores_internal_envelopes() {
    let state = State::default();
    handle_user_prompt(&json!({"session_id":"s", "prompt":"<task-notification>done", "cwd":"/tmp/p"}), &state).unwrap();
    handle_user_prompt(&json!({"session_id":"s", "prompt":"fix this", "cwd":"/tmp/p"}), &state).unwrap();
    assert_eq!(*state.prompts.borrow(), ["s:fix this"]);
}
#[test]
fn stop_defers_pending_work_and_filters_normal_completions() {
    let state = State {
        active: Some("work".into()),
        job: JobInfo { job_number: Some(1), duration_seconds: Some(11), prompt: Some("work".into()) },
        ..Default::default()
    };
    let mut sink = Sink::default();
    handle_stop(&json!({"session_id":"s", "background_tasks":[{}]}), &state, &config(), &mut sink).unwrap();
    assert!(state.stopped.borrow().is_empty());
    handle_stop(
        &json!({"session_id":"s", "cwd":"/tmp/project", "last_assistant_message":"done"}),
        &state,
        &config(),
        &mut sink,
    )
    .unwrap();
    assert_eq!(sink.0.borrow()[0].subtitle, "Claude completed in 11s");
}
#[test]
fn stop_without_an_active_turn_does_not_repeat_a_previous_completion() {
    let state = State {
        job: JobInfo { job_number: Some(1), duration_seconds: Some(11), prompt: Some("previous work".into()) },
        ..Default::default()
    };
    let mut sink = Sink::default();

    handle_stop(&json!({"session_id":"s"}), &state, &config(), &mut sink).unwrap();

    assert!(sink.0.borrow().is_empty());
}
#[test]
fn failure_bypasses_duration_and_prefix_filters_but_requires_all() {
    let state = State {
        active: Some("/skip work".into()),
        job: JobInfo { job_number: Some(1), duration_seconds: Some(1), prompt: Some("/skip work".into()) },
        ..Default::default()
    };
    let mut sink = Sink::default();
    let config = AppConfig {
        notification: NotificationConfig {
            threshold_seconds: 999,
            exclude_patterns: vec!["/skip".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    handle_stop_failure(&json!({"session_id":"s", "error":"nope"}), &state, &config, &mut sink).unwrap();
    assert_eq!(sink.0.borrow()[0].message, "Task: /skip work\nError: nope");
}
#[test]
fn idle_permission_and_question_contracts_are_preserved() {
    let state = State { active: Some("task".into()), ..Default::default() };
    let mut sink = Sink::default();
    handle_notification(&json!({"session_id":"s", "notification_type":"idle_prompt"}), &state, &config(), &mut sink)
        .unwrap();
    handle_permission(
        &json!({"session_id":"s", "tool_name":"Bash", "tool_input":{"command":"ls"}}),
        &state,
        &config(),
        &mut sink,
    )
    .unwrap();
    handle_ask_user_question(
        &json!({"session_id":"s", "tool_input":{"questions":[{"question":"Continue?"},{"question":"Why?"}]}}),
        &state,
        &config(),
        &mut sink,
    )
    .unwrap();
    assert_eq!(*state.waiting.borrow(), ["s"]);
    assert_eq!(sink.0.borrow()[0].message, "Task: task\nRequest: Bash — ls");
    assert_eq!(sink.0.borrow()[1].message, "Task: task\nQuestion: Continue? (+1 more)");
}
#[test]
fn codex_aliases_choose_last_user_message_and_have_no_duration_filter() {
    let mut sink = Sink::default();
    handle_codex_notify(&json!({"event":"agent-turn-complete", "cwd":"/tmp/p", "inputMessages":[{"role":"user", "content":[{"text":"first"}]},{"role":"user", "content":"last"}], "lastAssistantMessage":{"content":"done"}}), &config(), &mut sink).unwrap();
    assert_eq!(sink.0.borrow()[0].message, "Task: last\nResult: done");
}

#[test]
fn native_codex_hooks_keep_latest_prompt_and_notify_once_per_turn() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = config();
    config.database.path = directory.path().join("sessions.db");
    config.cleanup.auto_cleanup_enabled = false;
    config.notification.threshold_seconds = u64::MAX;
    let state = crate::state::SessionStore::new(&config).unwrap();
    let mut sink = Sink::default();
    for prompt in ["first request", "latest request"] {
        handle_codex_hook(
            &json!({"hook_event_name":"UserPromptSubmit", "session_id":"s", "turn_id":"t", "prompt":prompt}),
            &state,
            &config,
            &mut sink,
        )
        .unwrap();
    }
    assert!(sink.0.borrow().is_empty());
    let stop = json!({"hook_event_name":"Stop", "session_id":"s", "turn_id":"t", "cwd":"/tmp/project", "last_assistant_message":"done"});
    handle_codex_hook(&stop, &state, &config, &mut sink).unwrap();
    handle_codex_hook(&stop, &state, &config, &mut sink).unwrap();
    let notifications = sink.0.borrow();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].message, "Task: latest request\nResult: done");
    assert_eq!(notifications[0].subtitle, "Codex completed");
}

#[test]
fn native_codex_stop_preserves_modes_and_prompt_exclusions() {
    let stop = json!({"hook_event_name":"Stop", "session_id":"s", "turn_id":"t", "last_assistant_message":"done"});
    for (mode, prompt) in [
        (crate::model::NotificationMode::All, "/skip this"),
        (crate::model::NotificationMode::PermissionOnly, "work"),
        (crate::model::NotificationMode::Disabled, "work"),
    ] {
        let mut config = config();
        config.notification.mode = mode;
        config.notification.exclude_patterns = vec!["/skip".into()];
        let state = State { active: Some(prompt.into()), ..Default::default() };
        let mut sink = Sink::default();
        handle_codex_hook(&stop, &state, &config, &mut sink).unwrap();
        assert!(sink.0.borrow().is_empty());
        assert_eq!(*state.stopped.borrow(), ["codex:s:t"]);
    }
}

#[test]
fn native_codex_stop_without_prompt_state_still_reports_the_result() {
    let state = State::default();
    let mut sink = Sink::default();
    let mut stop = json!({"hook_event_name":"Stop", "session_id":"s", "turn_id":"t", "last_assistant_message":null});
    handle_codex_hook(&stop, &state, &config(), &mut sink).unwrap();
    assert_eq!(sink.0.borrow()[0].message, "Result: Turn completed.");
    stop["last_assistant_message"] = json!(42);
    assert!(handle_codex_hook(&stop, &state, &config(), &mut sink).is_err());
    stop["turn_id"] = json!("");
    assert!(handle_codex_hook(&stop, &state, &config(), &mut sink).is_err());
}

// Codex 0.155.1: tui/src/app/thread_title.rs::thread_title_instructions.
const CODEX_TITLE_INSTRUCTIONS: &str = "Generate a concise, single-line task title of at most \
    36 characters and under five words where possible. Start with an imperative verb. \
    Capitalize only the first word unless the user's language, proper nouns, acronyms, or code terms \
    require otherwise. Preserve ticket references exactly. Write in the user's language. \
    Do not use quotes, markdown, or trailing punctuation. Do not answer the request.";

#[test]
fn codex_internal_title_and_rename_turns_do_not_notify() {
    for context in [
        "\n\nUser prompt:\nFix the failing tests",
        "\nPrioritize the current task and latest substantive user request.\n\n\
         Recent conversation messages:\n<conversation>\n\
         <message role=\"user\">Fix the failing tests</message>\n</conversation>",
    ] {
        let mut sink = Sink::default();
        handle_codex_notify(
            &json!({
                "type": "agent-turn-complete",
                "client": "codex-tui",
                "input-messages": [format!("{CODEX_TITLE_INSTRUCTIONS}{context}")],
                "last-assistant-message": "{\"title\":\"Fix failing tests\"}",
            }),
            &config(),
            &mut sink,
        )
        .unwrap();
        assert!(sink.0.borrow().is_empty(), "internal title context: {context}");
    }
}

#[test]
fn codex_real_title_requests_and_quoted_internal_prompts_still_notify() {
    for prompt in [
        "Generate a title for my article".to_owned(),
        CODEX_TITLE_INSTRUCTIONS.to_owned(),
        format!("Explain this internal prompt: {CODEX_TITLE_INSTRUCTIONS}\n\nUser prompt:\nFix tests"),
    ] {
        let mut sink = Sink::default();
        handle_codex_notify(
            &json!({
                "type": "agent-turn-complete",
                "input-messages": [prompt],
                "last-assistant-message": "{\"title\":\"Fix failing tests\"}",
            }),
            &config(),
            &mut sink,
        )
        .unwrap();
        assert_eq!(sink.0.borrow().len(), 1, "user prompt: {prompt}");
    }
}

#[test]
fn duration_and_text_extractors_keep_historical_behavior() {
    assert_eq!(format_duration(3661), "1h1m");
    assert_eq!(failure_message(json!({"error_details":" a\n b "}).as_object().unwrap()), "a b");
    assert_eq!(extract_message_text(Some(&json!([{"text":"a"}, {"content":"b"}]))), "a b");
}

#[test]
fn codex_attention_uses_turn_context_and_preserves_completion_state() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = config();
    config.database.path = directory.path().join("sessions.db");
    config.cleanup.auto_cleanup_enabled = false;
    config.notification.threshold_seconds = u64::MAX;
    config.notification.exclude_patterns = vec!["/skip".into()];
    let state = crate::state::SessionStore::new(&config).unwrap();
    state.track_prompt("s", "Claude task", "");
    state.track_prompt("codex:s:other", "other turn", "");
    let mut sink = Sink::default();
    handle_codex_hook(
        &json!({"hook_event_name":"UserPromptSubmit", "session_id":"s", "turn_id":"t", "prompt":"/skip current task"}),
        &state,
        &config,
        &mut sink,
    )
    .unwrap();
    for mode in [crate::model::NotificationMode::All, crate::model::NotificationMode::PermissionOnly] {
        config.notification.mode = mode;
        handle_codex_hook(
            &json!({"hook_event_name":"PermissionRequest", "session_id":"s", "turn_id":"t", "cwd":"/tmp/project", "tool_name":"exec_command", "tool_input":{"command":"cargo test", "description":"Run tests"}}),
            &state,
            &config,
            &mut sink,
        )
        .unwrap();
        handle_codex_hook(
            &json!({"hook_event_name":"PreToolUse", "session_id":"s", "turn_id":"t", "tool_name":"request_user_input", "tool_input":{"questions":[{"question":" "}, {"question":"Which API?"}, {"question":"Which scope?"}]}}),
            &state,
            &config,
            &mut sink,
        )
        .unwrap();
    }
    let notifications = sink.0.borrow();
    assert_eq!(notifications.len(), 4);
    for pair in notifications.as_chunks::<2>().0 {
        assert_eq!(pair[0].client, Client::Codex);
        assert_eq!(pair[0].title, "project");
        assert_eq!(pair[0].subtitle, "Codex needs approval");
        assert_eq!(pair[0].message, "Task: /skip current task\nRequest: exec_command — cargo test");
        assert_eq!(pair[1].client, Client::Codex);
        assert_eq!(pair[1].title, "Codex");
        assert_eq!(pair[1].subtitle, "Codex needs input");
        assert_eq!(pair[1].message, "Task: /skip current task\nQuestion: Which API? (+1 more)");
    }
    assert_eq!(state.active_prompt("codex:s:t").as_deref(), Some("/skip current task"));
    assert_eq!(state.active_prompt("codex:s:other").as_deref(), Some("other turn"));
    assert_eq!(state.active_prompt("s").as_deref(), Some("Claude task"));
    assert_eq!(state.job_info("codex:s:t"), JobInfo::default());
}

#[test]
fn codex_attention_ignores_disabled_unrelated_and_malformed_questions() {
    let state = State { active: Some("task".into()), ..Default::default() };
    let mut config = config();
    let mut sink = Sink::default();
    let mut payload =
        json!({"hook_event_name":"PreToolUse", "session_id":"s", "turn_id":"t", "tool_name":"request_user_input"});
    for input in [
        Value::Null,
        json!({}),
        json!({"questions":"Continue?"}),
        json!({"questions":[]}),
        json!({"questions":[null, "Continue?", {"question":42}, {"question":" "}]}),
    ] {
        payload["tool_input"] = input;
        handle_codex_hook(&payload, &state, &config, &mut sink).unwrap();
    }
    payload["tool_input"] = json!({"questions":[{"question":"Continue?"}]});
    payload["tool_name"] = json!("exec_command");
    handle_codex_hook(&payload, &state, &config, &mut sink).unwrap();
    payload["tool_name"] = json!("request_user_input");
    config.notification.mode = crate::model::NotificationMode::Disabled;
    handle_codex_hook(&payload, &state, &config, &mut sink).unwrap();
    payload["hook_event_name"] = json!("PermissionRequest");
    handle_codex_hook(&payload, &state, &config, &mut sink).unwrap();
    assert!(sink.0.borrow().is_empty());
    assert!(state.prompts.borrow().is_empty());
    assert!(state.stopped.borrow().is_empty());
}

#[test]
fn codex_interrupt_silently_stops_only_its_tracked_turn() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = config();
    config.database.path = directory.path().join("sessions.db");
    config.cleanup.auto_cleanup_enabled = false;
    let state = crate::state::SessionStore::new(&config).unwrap();
    state.track_prompt("codex:s:t", "interrupted", "");
    state.track_prompt("codex:s:other", "still active", "");
    let mut sink = Sink::default();
    for turn in ["t", "untracked"] {
        let mut payload = json!({"hook_event_name":"Interrupt", "session_id":"s", "turn_id":turn});
        handle_codex_hook(&payload, &state, &config, &mut sink).unwrap();
        if turn == "t" {
            payload["hook_event_name"] = json!("Stop");
            handle_codex_hook(&payload, &state, &config, &mut sink).unwrap();
        }
    }
    assert!(sink.0.borrow().is_empty());
    assert!(state.active_prompt("codex:s:t").is_none());
    assert_eq!(state.job_info("codex:s:t").prompt.as_deref(), Some("interrupted"));
    assert_eq!(state.active_prompt("codex:s:other").as_deref(), Some("still active"));
    assert!(state.active_prompt("codex:s:untracked").is_none());
    assert_eq!(state.job_info("codex:s:untracked"), JobInfo::default());
}

#[test]
fn codex_new_hooks_retain_id_validation_and_unknown_event_errors() {
    let state = State::default();
    let mut sink = Sink::default();
    for event in ["PermissionRequest", "PreToolUse", "Interrupt", "Unknown"] {
        for (session, turn) in [(json!(""), json!("t")), (json!("s"), Value::Null), (json!("s"), json!(42))] {
            assert!(
                handle_codex_hook(
                    &json!({"hook_event_name":event, "session_id":session, "turn_id":turn}),
                    &state,
                    &config(),
                    &mut sink,
                )
                .is_err()
            );
        }
    }
    assert!(
        handle_codex_hook(
            &json!({"hook_event_name":"Unknown", "session_id":"s", "turn_id":"t"}),
            &state,
            &config(),
            &mut sink,
        )
        .is_err()
    );
    assert!(sink.0.borrow().is_empty());
}

#[test]
fn elicitation_observes_both_modes_with_task_without_url_or_schema_details() {
    let state = State { active: Some("/skip task".into()), ..Default::default() };
    let mut config = config();
    config.notification.threshold_seconds = u64::MAX;
    config.notification.exclude_patterns = vec!["/skip".into()];
    for mode in [crate::model::NotificationMode::All, crate::model::NotificationMode::PermissionOnly] {
        config.notification.mode = mode;
        let mut sink = Sink::default();
        for elicitation_mode in ["form", "url"] {
            handle_elicitation(
                &json!({"session_id":"s", "cwd":"/tmp/project", "mcp_server_name":"calendar", "message":"Choose a calendar", "mode":elicitation_mode, "url":"https://example.com/auth?token=private", "requested_schema":{"private":"schema"}}),
                &state,
                &config,
                &mut sink,
            )
            .unwrap();
        }
        for notification in sink.0.borrow().iter() {
            assert_eq!(notification.client, Client::Claude);
            assert_eq!(notification.title, "project");
            assert_eq!(notification.subtitle, "Claude needs input");
            assert_eq!(notification.message, "Task: /skip task\nMessage: calendar — Choose a calendar");
        }
        assert_eq!(sink.0.borrow().len(), 2);
    }
    assert!(state.stopped.borrow().is_empty());
    assert!(state.prompts.borrow().is_empty());
}

#[test]
fn elicitation_handles_missing_message_and_optional_session_but_respects_disabled() {
    let state = State::default();
    let mut config = config();
    let mut sink = Sink::default();
    let payload = json!({"mcp_server_name":"calendar", "message":" "});
    handle_elicitation(&payload, &state, &config, &mut sink).unwrap();
    assert_eq!(sink.0.borrow()[0].message, "Message: calendar — Input requested");
    config.notification.mode = crate::model::NotificationMode::Disabled;
    handle_elicitation(&payload, &state, &config, &mut sink).unwrap();
    assert_eq!(sink.0.borrow().len(), 1);
}

#[test]
fn actionable_claude_notifications_use_message_title_or_fallback_and_preserve_turns() {
    let state = State { active: Some("/skip task".into()), ..Default::default() };
    let mut config = config();
    config.notification.threshold_seconds = u64::MAX;
    config.notification.exclude_patterns = vec!["/skip".into()];
    for mode in [crate::model::NotificationMode::All, crate::model::NotificationMode::PermissionOnly] {
        config.notification.mode = mode;
        let mut sink = Sink::default();
        for (notification_type, message, title, expected) in [
            ("agent_needs_input", "Choose next step", "Ignored title", "Choose next step"),
            ("quota_auto_resume_stale", " ", "Quota expired", "Quota expired"),
            ("quota_auto_resume_disabled", "", "", "Quota auto-resume is disabled"),
        ] {
            handle_notification(
                &json!({"session_id":"s", "cwd":"/tmp/project", "notification_type":notification_type, "message":message, "title":title}),
                &state,
                &config,
                &mut sink,
            )
            .unwrap();
            let notifications = sink.0.borrow();
            let notification = notifications.last().unwrap();
            assert_eq!(notification.client, Client::Claude);
            assert_eq!(notification.title, "project");
            assert_eq!(notification.subtitle, "Claude needs input");
            assert_eq!(notification.message, format!("Task: /skip task\nMessage: {expected}"));
        }
        assert_eq!(sink.0.borrow().len(), 3);
    }
    assert_eq!(state.waiting.borrow().len(), 6);
    assert!(state.stopped.borrow().is_empty());
    assert!(state.prompts.borrow().is_empty());
}

#[test]
fn nonactionable_claude_notifications_are_silent_and_disabled_keeps_waiting_tracking() {
    let state = State::default();
    let mut config = config();
    let mut sink = Sink::default();
    for notification_type in [
        "idle_prompt",
        "agent_completed",
        "quota_auto_resume",
        "quota_auto_resume_fired",
        "elicitation_dialog",
        "elicitation_url_dialog",
        "permission_prompt",
        "unknown",
    ] {
        handle_notification(
            &json!({"session_id":"s", "notification_type":notification_type, "message":"waiting for input"}),
            &state,
            &config,
            &mut sink,
        )
        .unwrap();
    }
    config.notification.mode = crate::model::NotificationMode::Disabled;
    handle_notification(
        &json!({"session_id":"s", "notification_type":"agent_needs_input", "message":"Choose next step"}),
        &state,
        &config,
        &mut sink,
    )
    .unwrap();
    assert!(sink.0.borrow().is_empty());
    assert_eq!(state.waiting.borrow().len(), 9);
    assert!(state.stopped.borrow().is_empty());
}
