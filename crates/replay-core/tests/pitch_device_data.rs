//! 俯仰配件的数据锚定：核心常量必须与客户端 `devices.json` 的 122 定义一致，
//! 且**当前只有 122** 声明了炮管俯仰上限增量。
//!
//! 这不是内容统计：`upperPitchLimitIncrease` / `lowerPitchLimitIncrease` 是客户端
//! 配件脚本的语义字段。缺了这条锚定，上游改数值或新增第二件俯仰配件时，回放/射击
//! 会静默沿用旧语义；测试失败会强制显式复核并接入。

use std::path::PathBuf;

use serde_json::Value;
use wotb_replay_core::replay::combat::{
    EQ_IMPROVED_PITCH_LIMITS, PITCH_DEVICE_LOWER_DEG, PITCH_DEVICE_UPPER_DEG,
};

fn devices_json() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/research/devices.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读取 {} 失败: {e}", path.display()));
    serde_json::from_str(&text).expect("devices.json 必须是合法 JSON")
}

fn number(value: &Value) -> Option<f32> {
    value
        .as_f64()
        .map(|n| n as f32)
        .or_else(|| value.as_str().and_then(|s| s.parse::<f32>().ok()))
}

fn pitch_bonus(item: &Value) -> Option<(f32, f32)> {
    let bonus = item.get("script")?.get("params")?.get("bonusValues")?;
    let upper = number(bonus.get("upperPitchLimitIncrease")?)?;
    let lower = number(bonus.get("lowerPitchLimitIncrease")?)?;
    Some((upper, lower))
}

/// 递归扫描所有对象键：任一层出现俯仰增量键即视为声明了俯仰语义。
fn declares_pitch_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.contains_key("upperPitchLimitIncrease")
                || map.contains_key("lowerPitchLimitIncrease")
                || map.values().any(declares_pitch_key)
        }
        Value::Array(items) => items.iter().any(declares_pitch_key),
        _ => false,
    }
}

#[test]
fn device_122_matches_core_pitch_bonus_constants() {
    let root = devices_json();
    let devices = root["devices"].as_array().expect("devices[] 必须存在");
    let item = devices
        .iter()
        .find(|d| d["id"].as_u64() == Some(u64::from(EQ_IMPROVED_PITCH_LIMITS)))
        .expect("devices.json 必须包含 id=122");
    assert_eq!(
        item["stem"].as_str(),
        Some("improvedPitchLimitsSystem"),
        "122 的 stem 是核心语义的锚点"
    );
    let (upper, lower) = pitch_bonus(item).expect("122 必须声明 upper/lowerPitchLimitIncrease");
    assert_eq!(
        upper, PITCH_DEVICE_UPPER_DEG,
        "客户端 upperPitchLimitIncrease 与核心常量不一致"
    );
    assert_eq!(
        lower, PITCH_DEVICE_LOWER_DEG,
        "客户端 lowerPitchLimitIncrease 与核心常量不一致"
    );
}

#[test]
fn only_equipment_122_declares_pitch_limit_increases() {
    let root = devices_json();
    let devices = root["devices"].as_array().expect("devices[] 必须存在");
    let ids: Vec<u64> = devices
        .iter()
        .filter(|d| declares_pitch_key(d))
        .filter_map(|d| d["id"].as_u64())
        .collect();
    assert_eq!(
        ids,
        vec![u64::from(EQ_IMPROVED_PITCH_LIMITS)],
        "出现第二件俯仰配件（或 122 改名/改值）时必须显式复核并接入，不能静默忽略"
    );
}
