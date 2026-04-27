use crate::config::parser::ConfigFile;
use crate::daemon::commands::ShaderKind;
use crate::daemon::renderer::list_shader_names;
use serde_json::{Map, Value, json};

pub fn list_profiles_json(config: &ConfigFile, active: Option<&str>) -> String {
    let mut profiles: Map<String, Value> = Map::new();
    profiles.insert(
        "default".to_string(),
        serde_json::to_value(&config.default).unwrap_or(Value::Null),
    );
    for (name, cfg) in &config.additional {
        profiles.insert(
            name.clone(),
            serde_json::to_value(cfg).unwrap_or(Value::Null),
        );
    }
    serde_json::to_string(&json!({
        "active": active.unwrap_or("default"),
        "profiles": profiles,
    }))
    .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}

pub fn list_shaders_json(kind: &ShaderKind) -> String {
    let subdir = match kind {
        ShaderKind::Constant => "constant",
        ShaderKind::Transition => "transition",
    };
    let names = list_shader_names(subdir);
    serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string())
}
