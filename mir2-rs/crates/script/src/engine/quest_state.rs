//! 玩家任务态位图：`QuestFlag` / `QuestUnit` / `QuestUnitOpen`（各 `byte[128]`）。
//!
//! 逐行复刻 `src/M2Server/Player/PlayObject.cs:3966-4110` 的三对入口（三组数组语义**完全同形**，
//! oracle 只是把同一段代码复制了三遍）：
//!
//! | 数组 | get | set |
//! | --- | --- | --- |
//! | `QuestFlag` | `GetQuestFalgStatus`（`:3966`） | `SetQuestFlagStatus`（`:3990`） |
//! | `QuestUnitOpen` | `GetQuestUnitOpenStatus`（`:4013`） | `SetQuestUnitOpenStatus`（`:4037`） |
//! | `QuestUnit` | `GetQuestUnitStatus`（`:4060`） | `SetQuestUnitStatus`（`:4084`） |
//!
//! 位语义（oracle 原样；注意 `128 >> n` 是 **MSB-first**，不是 `1 << n`）：
//!
//! ```text
//! nFlag -= 1;                        // 脚本 flag 从 1 起；0/负数 ⇒ no-op / 返回 0
//! if (nFlag < 0) return;
//! n10 = nFlag / 8;                   // 字节下标（nFlag >= 0 ⇒ C# 与 Rust 整数除法同）
//! n14 = nFlag % 8;                   // 位号
//! if ((n10 - Length) < 0) {          // 越界（n10 >= 128）⇒ no-op / 返回 0
//!     bit = 128 >> n14;
//!     set: value == 0 ? (~bit & old) : (bit | old)
//!     get: (bit & old) != 0 ? 1 : 0
//! }
//! ```
//!
//! 真身向量：`mir2-rs/tests/parity/m3/quest-state-probe/vectors/quest_state_vectors.{json,tsv}`
//! （C# 探针对**真 `PlayObject`** 逐个边界调用上述六个入口产出）；Rust 用例
//! `crates/script/tests/quest_state_vectors.rs` 逐字段核对，含改坏必红。
//!
//! 会话载入/存档写回（`src/GameSrv/Word/WorldServer.cs:1160-1162` / `:1291-1293` 的
//! `playObject.Quest* = humData.Quest*`）不在本模块：它属 `crates/world`/`bins/game-svr`
//! 接线面，按 owner 08:10/08:20 的写集纪律停机交 owner（见本卡 patch body 的「写集边界」）。
//! `crates/storage`/`bins/game-svr` 的 `Quest*[128]` 编解码已由 M2 `record-fields` stage6 落地。

/// `PlayObject.QuestFlag/QuestUnit/QuestUnitOpen` 的数组长度（`PlayObject.cs:1038-1040`）。
pub const QUEST_ARRAY_LEN: usize = 128;

/// 一组任务标志位（对应 oracle 的 `byte[128]`）。
///
/// oracle 把同一段位运算复制在三组数组上；Rust 侧收成一个类型，由调用方持有三个实例
/// （登录时从会话记录载入、存档时写回），避免三份逐行重复实现各自漂移。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestBitmap {
    bytes: [u8; QUEST_ARRAY_LEN],
}

impl Default for QuestBitmap {
    fn default() -> Self {
        Self::new()
    }
}

impl QuestBitmap {
    /// 全零位图（对应 `new byte[128]`）。
    pub const fn new() -> Self {
        Self {
            bytes: [0u8; QUEST_ARRAY_LEN],
        }
    }

    /// 从会话记录里的 128 字节原位载入（登录：`playObject.Quest* = humData.Quest*`）。
    pub const fn from_bytes(bytes: [u8; QUEST_ARRAY_LEN]) -> Self {
        Self { bytes }
    }

    /// 只读原始 128 字节（存档写回 / 逐字节对拍用）。
    pub const fn as_bytes(&self) -> &[u8; QUEST_ARRAY_LEN] {
        &self.bytes
    }

    /// 取出原始 128 字节。
    pub const fn into_bytes(self) -> [u8; QUEST_ARRAY_LEN] {
        self.bytes
    }

    /// `GetQuestFalgStatus` / `GetQuestUnitStatus` / `GetQuestUnitOpenStatus`（三组同形）。
    ///
    /// 返回 `0` / `1`；`flag <= 0` 或字节下标越界一律 `0`。
    pub fn get(&self, n_flag: i32) -> i32 {
        let n_flag = n_flag - 1;
        if n_flag < 0 {
            return 0;
        }
        let n10 = n_flag / 8;
        let n14 = n_flag % 8;
        if (n10 - QUEST_ARRAY_LEN as i32) < 0 {
            let bit = 128u8 >> (n14 as u32);
            if (bit & self.bytes[n10 as usize]) != 0 {
                1
            } else {
                0
            }
        } else {
            0
        }
    }

    /// `SetQuestFlagStatus` / `SetQuestUnitStatus` / `SetQuestUnitOpenStatus`（三组同形）。
    ///
    /// `n_value == 0` 清目标位、否则置目标位；只改目标字节的目标位。
    /// `flag <= 0` 或字节下标越界一律 no-op。
    pub fn set(&mut self, n_flag: i32, n_value: i32) {
        let n_flag = n_flag - 1;
        if n_flag < 0 {
            return;
        }
        let n10 = n_flag / 8;
        let n14 = n_flag % 8;
        if (n10 - QUEST_ARRAY_LEN as i32) < 0 {
            let bit = 128u8 >> (n14 as u32);
            let bt15 = self.bytes[n10 as usize];
            self.bytes[n10 as usize] = if n_value == 0 {
                !bit & bt15
            } else {
                bit | bt15
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 逐行对照 `PlayObject.cs:3966-3988` 的位序：flag 107 ⇒ nFlag=106 ⇒ 字节 13、位 `128>>2 = 32`。
    #[test]
    fn flag_107_maps_to_byte_13_bit_32() {
        let mut b = QuestBitmap::new();
        assert_eq!(b.get(107), 0);
        b.set(107, 1);
        assert_eq!(b.as_bytes()[13], 32);
        assert_eq!(b.get(107), 1);
        b.set(107, 0);
        assert_eq!(b.as_bytes()[13], 0);
        assert_eq!(b.get(107), 0);
    }

    /// flag 0 / 负数 / 越界（> 1024）都是 no-op；flag 1024 落在最后一字节最低位。
    #[test]
    fn out_of_range_is_noop() {
        let mut b = QuestBitmap::new();
        for f in [0, -1, -100, 1025, 2048] {
            b.set(f, 1);
            assert_eq!(b.get(f), 0, "flag {f} 应越界 no-op");
        }
        for (i, byte) in b.as_bytes().iter().enumerate() {
            assert_eq!(*byte, 0, "字节 {i} 不应被越界写改动");
        }
        b.set(1024, 1);
        assert_eq!(b.as_bytes()[127], 1);
        assert_eq!(b.get(1024), 1);
    }

    /// `set(0)` 只清目标位，不碰同字节其它位（按位读改写语义）。
    #[test]
    fn clear_only_touches_target_bit() {
        let mut b = QuestBitmap::new();
        b.set(1, 1);
        b.set(8, 1);
        assert_eq!(b.as_bytes()[0], 0x81);
        b.set(1, 0);
        assert_eq!(b.as_bytes()[0], 0x01);
        assert_eq!(b.get(1), 0);
        assert_eq!(b.get(8), 1);
    }
}
