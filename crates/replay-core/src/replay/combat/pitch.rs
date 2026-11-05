//! 炮管俯仰极限模型（models.pb PitchExtremaInfo 的扇区插值）与 prop2 frac 解码。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 单扇区俯仰极值（度，models.pb PitchExtremaInfo）：min=−仰角上限、max=俯角上限、
/// range=扇区角宽（以正前 0°/正后 180° 为中心，±range/2）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorLimits {
    pub min: f32,
    pub max: f32,
    pub range: f32,
}

/// 一门主炮的俯仰限制（models.pb GunModelDefinition.pitch，顶级配置）：
/// 基础 (dep=俯角上限, ele=仰角上限) + 可选前/后扇区极值 + 过渡角。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GunPitchRange {
    /// 基础俯角上限（正值，度）——扇区外的全向值
    pub dep: f32,
    /// 基础仰角上限（正值，度）
    pub ele: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front: Option<SectorLimits>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back: Option<SectorLimits>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<f32>,
}

impl GunPitchRange {
    /// 叠加俯仰配件后的有效极限。当前唯一来源：装备 122「改进型垂直稳定器」
    /// （`data/research/devices.json`：upperPitchLimitIncrease=4、lowerPitchLimitIncrease=3）。
    /// 基础值与 front/back 扇区极值同增益（`dep`/`max` 加俯角、`ele`/`−min` 加仰角），
    /// `transition` 不变。基础与扇区同时平移，`gun_pitch_range_at` 的线性过渡带保持原形。
    pub fn with_pitch_bonus(&self, upper_deg: f32, lower_deg: f32) -> Self {
        let mut out = self.clone();
        out.dep += lower_deg;
        out.ele += upper_deg;
        if let Some(f) = out.front.as_mut() {
            f.max += lower_deg;
            f.min -= upper_deg;
        }
        if let Some(b) = out.back.as_mut() {
            b.max += lower_deg;
            b.min -= upper_deg;
        }
        out
    }
}

/// 昵称 → 炮管俯仰限制：prop2 frac 解码的锚定表。
/// 由 battle_results（昵称→tank_id）+ models.pb **实际搭载**炮塔/主炮
/// （`pitch_limits_from_battle_results` 的 comp blob 对号）构建；此处保存基础极限，
/// 实体实际携带的俯仰配件（122）由回放解码层按 eid 的 Type5 配件串叠加
/// （`effective_pitch_limits`），不在锚定表内预计算。
/// 注意：匿名玩家共用显示名 "Anonyme"，同场多个匿名玩家会互相覆盖（按昵称连接的固有歧义）。
pub type GunPitchLimits = HashMap<String, GunPitchRange>;

/// 炮塔相对角 θ（度，wrap ±180）处的有效 (俯角上限, 仰角上限)：
/// front 扇区以 0° 为中心、back 扇区以 180° 为中心（宽 range，各 ±range/2），扇区内用
/// 扇区极值、扇区外用基础值，过渡带（边界 ±transition/2）线性插值。
/// 过渡带几何为合理推断（BlitzKit applyPitchYawLimits 同名语义），transition 缺省 = 0。
fn gun_pitch_range_at(r: &GunPitchRange, turret_rel_deg: f32) -> (f32, f32) {
    let mut theta = turret_rel_deg;
    while theta > 180.0 {
        theta -= 360.0;
    }
    while theta < -180.0 {
        theta += 360.0;
    }
    let trans = r.transition.unwrap_or(0.0);
    // 单扇区混合：d = |θ−中心|，扇区内→扇区值，扇区外→基础值，边界两侧 ±trans/2 插值
    let blend = |base: f32, sect: f32, d: f32, half: f32| -> f32 {
        if half <= 0.0 {
            return base;
        }
        let lo = half - trans * 0.5;
        let hi = half + trans * 0.5;
        if d <= lo {
            return sect;
        }
        if d >= hi {
            return base;
        }
        let f = if hi > lo { (d - lo) / (hi - lo) } else { 1.0 };
        sect + (base - sect) * f
    };
    // 基础 (dep, ele)；front/back 扇区依次叠加（几何上不重叠，顺序无关）
    let mut dep = r.dep;
    let mut ele = r.ele;
    if let Some(f) = &r.front {
        dep = blend(dep, f.max, theta.abs(), f.range * 0.5);
        ele = blend(ele, -f.min, theta.abs(), f.range * 0.5);
    }
    if let Some(b) = &r.back {
        let d = (180.0 - theta.abs()).abs();
        dep = blend(dep, b.max, d, b.range * 0.5);
        ele = blend(ele, -b.min, d, b.range * 0.5);
    }
    (dep, ele)
}

/// prop2 frac6 → 炮管俯仰（弧度，炮塔系，正=仰角）。
/// **frac=63 ↔ 当前炮塔朝向的俯角极限（炮管最低）、frac=0 ↔ 仰角极限（最高）**：
/// pitch = ele(θ) − frac/63 × (dep(θ) + ele(θ))，其中 (dep,ele) 随炮塔相对角 θ 分段
/// （front/back 扇区，`gun_pitch_range_at`）。**比例锚定随炮塔朝向分段**：旋转一周
/// frac 恒钉 63 而后方扇区俯角远小于前方 = 服务器按当前朝向限制打包 frac
/// （受控实验极限动作 0/63 精确钳位 + 实战 frac 常态贴 63——第三人称预瞄点在
/// 近地面的操作常态）。快速偏航段 frac 恒钉极限 = 炮管贴极限的**实时如实上报**
/// （非通道冻结——服务器按需重发不变值）。
#[inline]
pub fn decode_prop2_gun_pitch(frac: f32, range: &GunPitchRange, turret_rel_rad: f32) -> f32 {
    let (dep, ele) = gun_pitch_range_at(range, turret_rel_rad * 57.29578);
    (ele - frac / 63.0 * (dep + ele)).to_radians()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> GunPitchRange {
        GunPitchRange {
            dep: 6.0,
            ele: 18.0,
            front: Some(SectorLimits {
                min: -20.0,
                max: 8.0,
                range: 60.0,
            }),
            back: Some(SectorLimits {
                min: -10.0,
                max: 2.0,
                range: 40.0,
            }),
            transition: Some(20.0),
        }
    }

    /// 122 的 +4° 仰 / +3° 俯必须同时作用于基础值与前后扇区；transition 不动。
    #[test]
    fn pitch_bonus_extends_base_and_sectors() {
        let b = sample();
        let e = b.with_pitch_bonus(4.0, 3.0);
        assert!((e.dep - 9.0).abs() < 1e-6);
        assert!((e.ele - 22.0).abs() < 1e-6);
        let f = e.front.expect("front 保留");
        assert!((f.max - 11.0).abs() < 1e-6, "扇区俯角 +3");
        assert!((f.min + 24.0).abs() < 1e-6, "扇区仰角 +4（min=−ele）");
        assert!((f.range - 60.0).abs() < 1e-6);
        let k = e.back.expect("back 保留");
        assert!((k.max - 5.0).abs() < 1e-6);
        assert!((k.min + 14.0).abs() < 1e-6);
        assert_eq!(e.transition, b.transition);
    }

    /// prop2 端点：frac=0（仰角上限）精确 +4°，frac=63（俯角上限）精确低 3°；
    /// 扇区朝向（0°/90°/180°/270°）都必须成立，中间 frac 由两端线性决定。
    #[test]
    fn pitch_bonus_moves_prop2_endpoints_exactly() {
        let b = sample();
        let e = b.with_pitch_bonus(4.0, 3.0);
        let d4 = 4.0f32.to_radians();
        let d3 = 3.0f32.to_radians();
        for rel_deg in [0.0f32, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0] {
            let rel = rel_deg.to_radians();
            let base_up = decode_prop2_gun_pitch(0.0, &b, rel);
            let base_down = decode_prop2_gun_pitch(63.0, &b, rel);
            let eq_up = decode_prop2_gun_pitch(0.0, &e, rel);
            let eq_down = decode_prop2_gun_pitch(63.0, &e, rel);
            assert!(
                (eq_up - (base_up + d4)).abs() < 1e-6,
                "rel={rel_deg}: frac=0 应 +4°"
            );
            assert!(
                ((base_down - eq_down) - d3).abs() < 1e-6,
                "rel={rel_deg}: frac=63 应低 3°"
            );
        }
    }

    /// 零加成是无操作（无 122 的既有场次必须逐值不变）。
    #[test]
    fn zero_bonus_is_identity() {
        let b = sample();
        let e = b.with_pitch_bonus(0.0, 0.0);
        assert_eq!(e.dep, b.dep);
        assert_eq!(e.ele, b.ele);
        assert_eq!(
            e.front.as_ref().expect("front").max,
            b.front.as_ref().expect("front").max
        );
        assert_eq!(
            e.front.as_ref().expect("front").min,
            b.front.as_ref().expect("front").min
        );
        assert_eq!(
            e.back.as_ref().expect("back").max,
            b.back.as_ref().expect("back").max
        );
        assert_eq!(
            e.back.as_ref().expect("back").min,
            b.back.as_ref().expect("back").min
        );
    }
}
