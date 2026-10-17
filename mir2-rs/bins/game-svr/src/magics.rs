//! 魔法模板表（`magics`）的**加载段** —— oracle `MySqlDB.LoadMagicDB`
//! （`src/GameSrv/DB/MySqlDB.cs:105`），由 `M2Share.MagicList` 持有、`PlayObject.GetMagicInfo`
//! 按 `MagicId` 查（`PlayObject.cs:4174-4185`）。
//!
//! 交付卡：`openmir2-m3-pet-slave-summon`（M3⑨-实现 2 第②片）。本片用它取 `DoSpell` 的
//! `Magic.Effect` / `Magic.EffectType`（`RM_SPELL`/`RM_MAGICFIRE` 帧参）与熟练度尾的
//! `Magic.TrainLv` / `TrainLevel[]` / `MaxTrain[]`。
//!
//! 加载失败 / 表为空 ⇒ 空表（调用方按 `GetMagicInfo == null` 降级：不接效果体）。**不伪造**。
//!
//! 与 [`crate::items`] 同一口径：直连 MySQL（oracle 在 `GameApp.Initialize` 直连读表，
//! 不经 DBSvr），连接串走 `LegacyDsn`（与 db-srv/login-srv 同一份解析）。

use std::collections::HashMap;

use mir2_data::loaders::DataLoaders;
use mir2_data::MagicInfo;
use mir2_storage::LegacyDsn;

/// 直连 MySQL 读 `magics`（`MySqlDB.LoadMagicDB`，`src/GameSrv/DB/MySqlDB.cs:105`）。
///
/// # Errors
/// 连接串解析失败、连库失败、`select * from magics` 失败时返回人话错误。
pub async fn load_magics(conn: &str) -> Result<Vec<MagicInfo>, String> {
    let dsn = LegacyDsn::parse(conn).map_err(|e| format!("ConnctionString 解析失败：{e}"))?;
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&dsn.to_sqlx_url())
        .await
        .map_err(|e| format!("连接 MySQL 失败（读 magics）：{e}"))?;
    // `monster_power_rate` 只被 `load_monster_db` 用，魔表不读它；照 `items` 的口径传 10。
    DataLoaders::new(pool, 10)
        .load_magic_db()
        .await
        .map_err(|e| format!("加载 magics 失败：{e}"))
}

/// `M2Share.MagicList` 的查表面：`MagicId → MagicInfo`。
///
/// oracle `GetMagicInfo(int nMagicId)` 线性扫 `UserMagic` 列表比 `Magic.MagicId`（`PlayObject.cs:4174-4185`，
/// 返回**首个**匹配）；`M2Share.MagicList` 按装表序构造（`WorldServer.cs:1193-1211`），
/// `WorldServer.FindMagic`（`:1430-1443`）亦取首个 ⇒ 重复 `MagicId` 必须**首行胜**。
/// **真魔表确有重复 MagID**（13 个；其中 48 的 `Spell/DefSpell` 不同：idx38 群体施毒术 12/30、
/// idx48 气功波 8/0）⇒ 表格构造按首行胜（旧实现 `insert` 后者覆盖会与 oracle 分歧，R 级数值差）。
#[derive(Debug, Clone, Default)]
pub struct MagicTable {
    by_id: HashMap<u16, MagicInfo>,
}

impl MagicTable {
    #[must_use]
    pub fn new(list: Vec<MagicInfo>) -> Self {
        let mut by_id = HashMap::with_capacity(list.len());
        for m in list {
            // 首行胜（oracle `FindMagic` 线性首匹配；装表序 = `M2Share.MagicList` 序）。
            by_id.entry(m.magic_id).or_insert(m);
        }
        Self { by_id }
    }

    /// `GameItemSystem.GetStdItem` 式的查表接缝：`MagicId → 模板`；缺列 `None`。
    #[must_use]
    pub fn get(&self, magic_id: u16) -> Option<&MagicInfo> {
        self.by_id.get(&magic_id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_looks_up_by_magic_id() {
        let t = MagicTable::new(vec![
            MagicInfo {
                magic_id: 17,
                magic_name: "召唤骷髅".into(),
                effect: 3,
                effect_type: 4,
                ..MagicInfo::default()
            },
            MagicInfo {
                magic_id: 20,
                ..MagicInfo::default()
            },
        ]);
        assert_eq!(t.len(), 2);
        assert_eq!(t.get(17).map(|m| m.effect), Some(3));
        assert_eq!(t.get(20).map(|m| m.effect_type), Some(0));
        assert_eq!(t.get(99), None);
        assert!(MagicTable::default().is_empty());
    }

    /// 重复 `MagicId` 必须**首行胜**：oracle `WorldServer.FindMagic`（`WorldServer.cs:1430-1443`）
    /// 线性扫 `MagicList`（按装表序，`:1193-1211`）取首个匹配。真魔表 48 号有两条且
    /// `Spell/DefSpell` 不同（idx38 群体施毒术 12/30、idx48 气功波 8/0）⇒ MP 包装层的
    /// `GetSpellPoint` 必须用首行，否则 MP 数值/失败门（R 级）分歧。
    #[test]
    fn duplicate_magic_id_first_row_wins() {
        let t = MagicTable::new(vec![
            MagicInfo {
                magic_id: 48,
                magic_name: "群体施毒术".into(),
                spell: 12,
                def_spell: 30,
                ..MagicInfo::default()
            },
            MagicInfo {
                magic_id: 48,
                magic_name: "气功波".into(),
                spell: 8,
                def_spell: 0,
                ..MagicInfo::default()
            },
        ]);
        assert_eq!(t.len(), 1, "同 MagicId 只留一条");
        let m = t.get(48).expect("48 必须在表内");
        assert_eq!(m.magic_name, "群体施毒术", "oracle FindMagic 首匹配");
        assert_eq!((m.spell, m.def_spell), (12, 30), "首行 Spell/DefSpell");
    }
}
