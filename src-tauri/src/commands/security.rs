use std::sync::Arc;
use tauri::State;

use crate::AppState;
use crate::security::models::{
    BuiltinRule, CreateCustomRuleInput, CustomRule, UpdateBuiltinRuleInput,
};
use crate::security::rules::{BuiltinRuleRepository, CustomRuleRepository};

// ---------- 内置规则命令 ----------

#[tauri::command]
pub async fn get_builtin_security_rules(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<BuiltinRule>, String> {
    BuiltinRuleRepository::get_all(&state.db.pool)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_builtin_security_rule(
    id: String,
    input: UpdateBuiltinRuleInput,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    BuiltinRuleRepository::update(&state.db.pool, &id, &input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reset_builtin_security_rules(
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    BuiltinRuleRepository::reset_to_defaults(&state.db.pool)
        .await
        .map_err(|e| e.to_string())
}

// ---------- 自定义规则命令 ----------

#[tauri::command]
pub async fn get_custom_security_rules(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<CustomRule>, String> {
    CustomRuleRepository::get_all(&state.db.pool)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_custom_security_rule(
    input: CreateCustomRuleInput,
    state: State<'_, Arc<AppState>>,
) -> Result<CustomRule, String> {
    // 后端兜底校验：category 只认这 4 个值，写错会导致匹配静默失效
    const VALID_CATEGORIES: [&str; 4] = ["domain", "tool", "path", "keyword"];
    if !VALID_CATEGORIES.contains(&input.category.as_str()) {
        return Err(format!(
            "非法的 category: {}，可选值: {}",
            input.category,
            VALID_CATEGORIES.join(", ")
        ));
    }
    if input.rule_type != "blacklist" && input.rule_type != "whitelist" {
        return Err(format!("非法的 rule_type: {}", input.rule_type));
    }
    CustomRuleRepository::create(&state.db.pool, &input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn toggle_custom_security_rule(
    id: String,
    enabled: bool,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    CustomRuleRepository::update_enabled(&state.db.pool, &id, enabled)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_custom_security_rule(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    CustomRuleRepository::delete(&state.db.pool, &id)
        .await
        .map_err(|e| e.to_string())
}
