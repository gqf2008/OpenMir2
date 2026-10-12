//! 由装备集算「人类外形 feature 位」——`GetFeatureToLong()` / `GetFeatureEx()` 的可复用纯函数接缝。
//!
//! # 这条接缝给谁用
//!
//! oracle 在 `PlayObject.SendLogon()`（`src/M2Server/Player/PlayObject.cs:1128-1147`）**同组**发三条入图帧，
//! 其中两条的载荷直接由本模块产出：
//!
//! | 帧 | ident | 用到本模块的哪个值 |
//! | --- | --- | --- |
//! | 50 | `SM_LOGON` | `MessageBodyWL.Param1 = GetFeatureToLong()` |
//! | 41 | `SM_FEATURECHANGED` | `nRecog = GetFeatureToLong()`（`LoWord`→Param、`HiWord`→Tag）、`Series = GetFeatureEx()` |
//! | 213 | `SM_ATTACKMODE` | （与 feature 无关，仅同组） |
//!
//! 同一对值也用在**装备/卸下成功的回帧**上（`PlayObject.Operate.cs:458` 的 `SM_TAKEON_OK`、
//! `:547` 的 `SM_TAKEOFF_OK`），所以它不是"只给 M2 用"的一次性件。
//!
//! # oracle 链（逐行对齐）
//!
//! ```text
//! M2Server.Player.PlayObject.SendLogon()            PlayObject.cs:1128
//!   └─ BaseObject.GetFeatureToLong()                BaseObject.cs:1487
//!        └─ PlayObject.GetFeature(null)             PlayObject.cs:3393
//!             ├─ nDress  = (byte)(StdItem(UseItems[Dress].Index ).Shape * 2) + (byte)Gender
//!             ├─ nWeapon = (byte)Gender + (byte)(StdItem(UseItems[Weapon].Index).Shape * 2)
//!             ├─ nHair   = (byte)(Hair * 2 + (byte)Gender)
//!             └─ M2Share.MakeHumanFeature(raceImg:0, nDress, nWeapon, nHair)   M2Share.cs:1965
//!                  = MakeLong(MakeWord(raceImg, weapon), MakeWord(hair, dress))
//! ```
//!
//! 细节（都会改变结果，别"顺手简化"）：
//!
//! 1. `HUtil32.MakeWord(byte low, byte high)` = `high << 8 | low`（**参数序：低字节在前**）；
//!    `HUtil32.MakeLong(ushort lowPart, ushort highPart)` = `lowPart | highPart << 16`。
//!    ⇒ `feature` 的 int32 位布局（LSB→MSB）= **`[raceImg:8][weapon:8][hair:8][dress:8]`**。
//! 2. `(byte)(StdItem.Shape * 2)` 与后面的 `byte +=` 都是 **byte 截断**：`Shape=200 ⇒ 144`、
//!    `Hair=255, gender=0 ⇒ 254`。这里用 `wrapping_*` 复刻。
//! 3. 衣服/武器槽只有在 `UseItems[slot] != null && .Index > 0` **且** `GetStdItem(Index) != null` 时才计入；
//!    其余一律按 0 处理（本模块用 `Option<u8>` 表达"该槽是否计入 Shape"）。
//! 4. `CharacterObject.GetFeatureEx()`（`CharacterObject.cs:146-149`）
//!    = `MakeWord(OnHorse ? HorseType : 0, DressEffType)`
//!    ⇒ int16 位布局（LSB→MSB）= **`[horse:8][dressEff:8]`**。
//!    **实测**：oracle 里 `DressEffType`/`HorseType` 全仓只有声明与读取，没有任何赋值点
//!    （`grep -rn "DressEffType|HorseType" src/` 只命中声明与 `CharacterObject.cs:148`）
//!    ⇒ 现网 `GetFeatureEx()` 恒为 0；本函数仍按语义实现，M2 车道接线时不要硬编码 0。
//!
//! # 与 oracle 逐位相同怎么保证
//!
//! 用例值由 `mir2-rs/tests/parity/m3/feature-probe`（引用 `src/M2Server` 真身）实测产出，
//! 落 `mir2-rs/tests/parity/m3/fixtures/feature_vectors.json`；本模块的测试回读该夹具逐位核对
//! （改坏任一位 ⇒ 必红）。生成命令：
//!
//! ```text
//! dotnet run --project mir2-rs/tests/parity/m3/feature-probe -c Release -- \
//!   mir2-rs/tests/parity/m3/fixtures/feature_vectors.json
//! ```

/// `HUtil32.MakeWord(byte low, byte high)`（`src/OpenMir2/HUtil32.cs:61`）= `(ushort)(high << 8 | low)`。
///
/// **参数序是（低字节, 高字节）**——oracle 里 `MakeWord(raceImg, weapon)` 得到的是 `weapon << 8 | raceImg`。
#[must_use]
pub const fn make_word(low: u8, high: u8) -> u16 {
    ((high as u16) << 8) | (low as u16)
}

/// `HUtil32.MakeLong(ushort lowPart, ushort highPart)`（`src/OpenMir2/HUtil32.cs:56`）= `lowPart | highPart << 16`。
#[must_use]
pub const fn make_long(low: u16, high: u16) -> i32 {
    ((high as i32) << 16) | (low as i32)
}

/// `HUtil32.LoWord`：取低 16 位（`SM_FEATURECHANGED` 的 `Param`）。
#[must_use]
pub const fn lo_word(value: i32) -> u16 {
    (value & 0xFFFF) as u16
}

/// `HUtil32.HiWord`：取高 16 位（`SM_FEATURECHANGED` 的 `Tag`）。
#[must_use]
pub const fn hi_word(value: i32) -> u16 {
    ((value >> 16) & 0xFFFF) as u16
}

/// `M2Share.MakeHumanFeature(byte raceImg, byte dress, byte weapon, byte hair)`（`src/M2Server/M2Share.cs:1965`）：
/// `MakeLong(MakeWord(raceImg, weapon), MakeWord(hair, dress))`。
#[must_use]
pub const fn make_human_feature(race_img: u8, dress: u8, weapon: u8, hair: u8) -> i32 {
    make_long(make_word(race_img, weapon), make_word(hair, dress))
}

/// `M2Share.MakeMonsterFeature(byte raceImg, byte weapon, ushort appr)`
/// （`src/M2Server/M2Share.cs:1960`）：`MakeLong(MakeWord(raceImg, weapon), appr)`。
///
/// 怪物的 `GetFeature()`（`BaseObject.Base.cs:911`）/`GetFeatureToLong()`（`BaseObject.cs:1487`）
/// 都走它；输入全部来自 `MonsterInfo.RaceImg` / `MonsterInfo.Appr` 与
/// `cert.MonsterWeapon = HUtil32.LoByte(monster.MP)`（`WorldServer.MonGen.cs:902`）。
///
/// 位布局（LSB→MSB）= **`[raceImg:8][weapon:8][appr:16]`**（与 `make_human_feature` 的
/// `[raceImg][weapon][hair][dress]` 不同——别把两个搞混）。
#[must_use]
pub const fn make_monster_feature(race_img: u8, monster_weapon: u8, appr: u16) -> i32 {
    make_long(make_word(race_img, monster_weapon), appr)
}

/// oracle `GetFeature` 里的 `nDress`：`(byte)(Shape * 2) + (byte)Gender`，全按 byte 截断。
///
/// `dress_shape = None` 表示该槽未计入（未穿 / `Index == 0` / `GetStdItem` 返回 null）。
#[must_use]
pub const fn dress_byte(dress_shape: Option<u8>, gender: u8) -> u8 {
    match dress_shape {
        Some(shape) => shape.wrapping_mul(2).wrapping_add(gender),
        None => gender,
    }
}

/// oracle `GetFeature` 里的 `nWeapon`：`(byte)Gender + (byte)(Shape * 2)`，全按 byte 截断。
#[must_use]
pub const fn weapon_byte(weapon_shape: Option<u8>, gender: u8) -> u8 {
    match weapon_shape {
        Some(shape) => gender.wrapping_add(shape.wrapping_mul(2)),
        None => gender,
    }
}

/// oracle `GetFeature` 里的 `nHair`：`(byte)(Hair * 2 + (byte)Gender)`。
#[must_use]
pub const fn hair_byte(hair: u8, gender: u8) -> u8 {
    hair.wrapping_mul(2).wrapping_add(gender)
}

/// `PlayObject.GetFeature(null)`（`PlayObject.cs:3393`）——角色的外形 feature 位（`SM_LOGON.Param1` /
/// `SM_FEATURECHANGED.nRecog`）。
///
/// `dress_shape` / `weapon_shape` 传 `UseItems[ItemLocation::Dress/Weapon]` 对应 `StdItem.Shape`；
/// 槽为空 / `Index == 0` / `GetStdItem` 返回 null 时传 `None`。
#[must_use]
pub const fn human_feature(
    dress_shape: Option<u8>,
    weapon_shape: Option<u8>,
    gender: u8,
    hair: u8,
) -> i32 {
    make_human_feature(
        0,
        dress_byte(dress_shape, gender),
        weapon_byte(weapon_shape, gender),
        hair_byte(hair, gender),
    )
}

/// `CharacterObject.GetFeatureEx()`（`CharacterObject.cs:146`）=
/// `MakeWord(OnHorse ? HorseType : 0, DressEffType)`。
///
/// 位布局（LSB→MSB）= `[horse:8][dressEff:8]`。现网 oracle 的 `DressEffType` / `HorseType`
/// 无任何赋值点（见模块文档），故恒为 0；这里不硬编码，按语义保留输入。
#[must_use]
pub const fn feature_ex(on_horse: bool, horse_type: u8, dress_eff_type: u8) -> u16 {
    let horse = if on_horse { horse_type } else { 0 };
    make_word(horse, dress_eff_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// oracle 实测向量（`mir2-rs/tests/parity/m3/feature-probe` 产出）。
    fn fixture() -> Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/parity/m3/fixtures/feature_vectors.json"
        );
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("oracle 向量夹具缺失/不可读：{path}（{e}）—— 不许静默跳过"));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path} 不是合法 JSON：{e}"))
    }

    /// 位布局锚点（LSB→MSB = raceImg, weapon, hair, dress）——改坏字段序必红。
    #[test]
    fn human_feature_byte_layout_is_race_img_weapon_hair_dress() {
        let f = make_human_feature(7, 6, 5, 4);
        let bytes = f.to_le_bytes();
        assert_eq!(
            bytes,
            [7, 5, 4, 6],
            "layout: [raceImg][weapon][hair][dress]"
        );
        assert_eq!(lo_word(f), 0x0507, "低字 = weapon<<8|raceImg");
        assert_eq!(hi_word(f), 0x0604, "高字 = dress<<8|hair");
    }

    /// `MakeMonsterFeature` 的位布局（LSB→MSB = raceImg, weapon, appr:lo, appr:hi）——
    /// 改坏字段序必红。
    #[test]
    fn monster_feature_byte_layout_is_race_img_weapon_then_appr() {
        let f = make_monster_feature(0x11, 0x22, 0x3344);
        assert_eq!(
            f.to_le_bytes(),
            [0x11, 0x22, 0x44, 0x33],
            "layout: [raceImg][weapon][appr.lo][appr.hi]"
        );
        assert_eq!(lo_word(f), 0x2211, "低字 = weapon<<8|raceImg");
        assert_eq!(hi_word(f), 0x3344, "高字 = appr");
    }

    /// MakeWord 的参数序：`MakeWord(low, high) = high << 8 | low`。
    #[test]
    fn make_word_puts_second_arg_in_high_byte() {
        assert_eq!(make_word(0x12, 0x34), 0x3412);
        assert_eq!(make_word(0x00, 0x80), 0x8000);
    }

    /// A. 端到端：oracle `PlayObject.GetFeature(null)` 的 16 个实测用例逐位相同。
    #[test]
    fn matches_oracle_fixture_cases() {
        let doc = fixture();
        let cases = doc["cases"].as_array().expect("cases 数组");
        assert_eq!(cases.len(), 16, "夹具用例数变了？先重新生成向量");
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let gender = case["gender"].as_u64().unwrap() as u8;
            let hair = case["hair"].as_u64().unwrap() as u8;
            let dress = case["dress_shape"].as_u64().map(|v| v as u8);
            let weapon = case["weapon_shape"].as_u64().map(|v| v as u8);
            let expected = case["feature"].as_i64().unwrap() as i32;
            let actual = human_feature(dress, weapon, gender, hair);
            assert_eq!(
                actual, expected,
                "{name}: human_feature({dress:?}, {weapon:?}, {gender}, {hair}) 与 oracle 不符"
            );
            assert_eq!(
                case["feature_hex"].as_str().unwrap(),
                format!("0x{actual:08X}"),
                "{name}: feature_hex 与 feature 不自洽"
            );
        }
    }

    /// B. 原语：`MakeHumanFeature(raceImg, dress, weapon, hair)` 的 8 个实测向量逐位相同。
    #[test]
    fn matches_oracle_fixture_packing() {
        let doc = fixture();
        let packing = doc["packing"].as_array().expect("packing 数组");
        assert_eq!(packing.len(), 8);
        for p in packing {
            let race_img = p["race_img"].as_u64().unwrap() as u8;
            let dress = p["dress"].as_u64().unwrap() as u8;
            let weapon = p["weapon"].as_u64().unwrap() as u8;
            let hair = p["hair"].as_u64().unwrap() as u8;
            let expected = p["value"].as_i64().unwrap() as i32;
            assert_eq!(
                make_human_feature(race_img, dress, weapon, hair),
                expected,
                "MakeHumanFeature({race_img}, {dress}, {weapon}, {hair}) 与 oracle 不符"
            );
        }
    }

    /// C. `GetFeatureEx()` 的 6 个实测向量逐位相同。
    #[test]
    fn matches_oracle_fixture_feature_ex() {
        let doc = fixture();
        let ex = doc["feature_ex"].as_array().expect("feature_ex 数组");
        assert_eq!(ex.len(), 6);
        for e in ex {
            let name = e["name"].as_str().unwrap();
            let on_horse = e["on_horse"].as_bool().unwrap();
            let horse_type = e["horse_type"].as_u64().unwrap() as u8;
            let dress_eff = e["dress_eff_type"].as_u64().unwrap() as u8;
            let expected = e["value"].as_u64().unwrap() as u16;
            assert_eq!(
                feature_ex(on_horse, horse_type, dress_eff),
                expected,
                "{name}: feature_ex({on_horse}, {horse_type}, {dress_eff}) 与 oracle 不符"
            );
        }
    }

    /// 金标准锚点：M0 抓包 `SM_FEATURECHANGED` 的 `nRecog = 0x00020000`
    /// ↔ 裸男 `Hair=1`（无衣服/武器）。这把"接缝 ↔ 真机帧"钉在一起。
    #[test]
    fn golden_featurechanged_nrecog_is_reproduced() {
        assert_eq!(human_feature(None, None, 0, 1), 0x0002_0000);
        // 帧 41 的 nRecog 会拆成 Lo/Hi 两个 word 落到 Param/Tag。
        let f = human_feature(None, None, 0, 1);
        assert_eq!((lo_word(f), hi_word(f)), (0x0000, 0x0002));
    }

    /// byte 截断（`(byte)(Shape*2)` / `byte +=`）：`Shape=200 ⇒ 144`、`Hair=255 ⇒ 254`。
    #[test]
    fn shape_and_hair_arithmetic_truncates_to_byte_like_oracle() {
        // 衣服 Shape=200：nDress = (byte)400 = 144 ⇒ 落在高字节（dress）。
        assert_eq!(human_feature(Some(200), None, 0, 1), 0x9002_0000u32 as i32);
        // 武器 Shape=200：nWeapon = 144 ⇒ 落在 bit8..15。
        assert_eq!(human_feature(None, Some(200), 0, 1), 0x0002_9000);
        // Hair=255：nHair = (byte)510 = 254。
        assert_eq!(human_feature(None, None, 0, 255), 0x00FE_0000);
    }

    /// 未穿槽（`None`）不改变 feature——对应 oracle 的"Index==0 / GetStdItem==null 一律按 0"。
    #[test]
    fn absent_slots_contribute_zero() {
        let base = human_feature(None, None, 1, 2);
        assert_eq!(dress_byte(None, 1), 1);
        assert_eq!(weapon_byte(None, 1), 1);
        assert_eq!(human_feature(None, None, 1, 2), base);
        assert_ne!(human_feature(Some(1), None, 1, 2), base);
        assert_ne!(human_feature(None, Some(1), 1, 2), base);
    }

    /// 现网的 `GetFeatureEx()` 恒为 0（`DressEffType`/`HorseType` 在 oracle 里无赋值点）——
    /// 这是"记录在案"的行为，不是巧合：接线时别把它当成必然。
    #[test]
    fn shipped_feature_ex_is_zero_for_unset_inputs() {
        assert_eq!(feature_ex(false, 0, 0), 0);
        // 骑乘态只有 HorseType 非 0 时才有位（现网无人赋值 ⇒ 仍为 0）。
        assert_eq!(feature_ex(true, 0, 0), 0);
    }
}
