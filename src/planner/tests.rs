use super::Plan;
use super::plan;
use super::prompt::PLANNER_SYSTEM_PROMPT;
use super::session::interactive_plan_with_resolver;
use super::types::QuestionResolver;

use anyhow::Result;
use httpmock::prelude::*;
use serde_json::json;

use crate::{
    client::{AIClient, ChatMessage, ChatMessageRole},
    config::{Config, LlmProvider, LlmSettings, ModelSettings, RecoverySettings},
    tokens::compute_completion_token_budget,
};

fn sample_config() -> Config {
    Config {
        llm: LlmSettings {
            provider: LlmProvider::OpenRouter,
            api_key: "test-key".to_string(),
            timeout_secs: 30,
            base_url: "https://openrouter.ai/api/v1".to_string(),
            user_agent: "li/test".to_string(),
        },
        models: ModelSettings {
            planner: "minimax/minimax-m2:free".to_string(),
            max_tokens: 512,
        },
        recovery: RecoverySettings::default(),
    }
}

fn expected_request_body(user_input: &str) -> serde_json::Value {
    let messages = vec![
        ChatMessage {
            role: ChatMessageRole::System,
            content: PLANNER_SYSTEM_PROMPT.to_string(),
        },
        ChatMessage {
            role: ChatMessageRole::User,
            content: user_input.to_string(),
        },
    ];
    let max_tokens = compute_completion_token_budget(512, &messages);

    json!({
        "model": "minimax/minimax-m2:free",
        "messages": [
            {
                "role": "system",
                "content": PLANNER_SYSTEM_PROMPT
            },
            {
                "role": "user",
                "content": user_input
            }
        ],
        "max_tokens": max_tokens,
        "temperature": 0.0
    })
}

async fn plan_with_resolver(
    client: &AIClient,
    request: &str,
    model: &str,
    max_tokens: u32,
    resolver: &QuestionResolver,
) -> Result<Plan> {
    interactive_plan_with_resolver(client, request, model, max_tokens, resolver).await
}

#[tokio::test]
async fn plan_parses_valid_response() {
    let server = MockServer::start_async().await;

    let _mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .header("Authorization", "Bearer test-key")
                .json_body(expected_request_body("make a new git repo"));

            then.status(200).json_body(json!({
                "choices": [
                    {
                        "index": 0,
                        "finish_reason": "stop",
                        "message": {
                            "role": "assistant",
                            "content": "{\"type\":\"plan\",\"confidence\":0.82,\"dry_run_commands\":[\"git status\"],\"execute_commands\":[\"git init\",\"git add .\",\"git commit -m \\\"Initial commit\\\"\"],\"notes\":\"Created minimal git repo with initial commit.\"}"
                        }
                    }
                ]
            }));
        })
        .await;

    let mut config = sample_config();
    config.llm.base_url = server.url("/v1");
    let client = AIClient::new(&config.llm).unwrap();

    let plan = plan(
        &client,
        "make a new git repo",
        &config.models.planner,
        config.models.max_tokens,
    )
    .await
    .unwrap();

    assert!((plan.confidence - 0.82).abs() < f32::EPSILON);
    assert_eq!(plan.dry_run_commands, vec!["git status".to_string()]);
    assert_eq!(
        plan.execute_commands,
        vec![
            "git init".to_string(),
            "git add .".to_string(),
            "git commit -m \"Initial commit\"".to_string()
        ]
    );
    assert_eq!(plan.notes, "Created minimal git repo with initial commit.");

    _mock.assert_async().await;
}

#[tokio::test]
async fn plan_handles_question_response() {
    let server = MockServer::start_async().await;

    let _mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .header("Authorization", "Bearer test-key")
                .json_body(expected_request_body("create a remote git repo"));

            then.status(200).json_body(json!({
                "choices": [
                    {
                        "index": 0,
                        "finish_reason": "stop",
                        "message": {
                            "role": "assistant",
                            "content": "{\"type\":\"question\",\"text\":\"What server should I use for the remote repository?\",\"context\":\"Creating a remote git repository\"}"
                        }
                    }
                ]
            }));
        })
        .await;

    let mut config = sample_config();
    config.llm.base_url = server.url("/v1");
    let client = AIClient::new(&config.llm).unwrap();

    let resolver = |question: &str, _context: &str| {
        assert_eq!(
            question,
            "What server should I use for the remote repository?"
        );
        Ok("skip".to_string())
    };

    let result = plan_with_resolver(
        &client,
        "create a remote git repo",
        &config.models.planner,
        config.models.max_tokens,
        &resolver,
    )
    .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Planning cancelled by user"));

    _mock.assert_async().await;
}

#[tokio::test]
async fn plan_errors_on_missing_fields() {
    let server = MockServer::start_async().await;

    let _mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .header("Authorization", "Bearer test-key");

            then.status(200).json_body(json!({
                "choices": [
                    {
                        "index": 0,
                        "finish_reason": "stop",
                        "message": {
                            "role": "assistant",
                            "content": "{}"
                        }
                    }
                ]
            }));
        })
        .await;

    let mut config = sample_config();
    config.llm.base_url = server.url("/v1");
    let client = AIClient::new(&config.llm).unwrap();

    let err = plan(
        &client,
        "make a new git repo",
        &config.models.planner,
        config.models.max_tokens,
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("Failed to parse planner JSON"));
    _mock.assert_async().await;
}
