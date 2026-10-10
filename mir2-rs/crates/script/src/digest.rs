//! 解析产物的结构摘要：把 `ScriptInfo` 全量内容压成确定性指纹，用于双侧逐文件对拍。
//!
//! 动机（A 线审查 F1）：只比计数与错误行文本，会让“结构对、内容错”的缺陷（命令码位移、
//! 字段归属、重名 label 改名序列）静默通过。本摘要把 label（含改名后缀）、每条条件/动作的
//! 命令码、六个字符串参数、六个数值参数、opname 全部纳入。
//!
//! 编码格式（C# harness 侧逐字节复刻）：
//! - 字符串统一 UTF-8，写成 `len:value`（`len` 为 UTF-8 字节数，十进制）；
//! - 字段分隔符 `|`；标签：`S`=脚本头、`R`=记录、`P`=过程、`C`=条件、`A`=动作、`E`=否则动作；
//! - 顺序：脚本 → 记录（插入序）→ 过程（插入序）→ 条件 → 动作 → 否则动作。
//!
//! 哈希：FNV-1a 64（非加密用途，仅用于漂移检测；两实现内联同一算法，无额外依赖）。

use crate::model::{Goods, MerchantFlags, QuestActionInfo, QuestConditionInfo, ScriptInfo};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// 结构计数（与摘要一同输出，便于人工比对差异位置）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StructureCounts {
    pub scripts: usize,
    pub records: usize,
    pub procedures: usize,
    pub conditions: usize,
    pub actions: usize,
    pub else_actions: usize,
}

/// 解析产物的结构指纹。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureDigest {
    pub hash: String,
    pub counts: StructureCounts,
}

struct Hasher {
    state: u64,
}

impl Hasher {
    fn new() -> Self {
        Self { state: FNV_OFFSET }
    }

    fn bytes(&mut self, b: &[u8]) {
        for x in b {
            self.state ^= *x as u64;
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
    }

    fn num(&mut self, n: i32) {
        self.bytes(n.to_string().as_bytes());
    }

    /// `len:value`（len 为 UTF-8 字节数）
    fn string(&mut self, s: &str) {
        self.bytes(s.len().to_string().as_bytes());
        self.bytes(b":");
        self.bytes(s.as_bytes());
    }

    fn sep(&mut self) {
        self.bytes(b"|");
    }
}

fn hash_condition(h: &mut Hasher, c: &QuestConditionInfo, counts: &mut StructureCounts) {
    h.bytes(b"C");
    h.sep();
    h.num(c.cmd_code);
    h.sep();
    for p in [
        &c.s_param1,
        &c.s_param2,
        &c.s_param3,
        &c.s_param4,
        &c.s_param5,
        &c.s_param6,
    ] {
        h.string(p);
        h.sep();
    }
    for n in [
        c.n_param1, c.n_param2, c.n_param3, c.n_param4, c.n_param5, c.n_param6,
    ] {
        h.num(n);
        h.sep();
    }
    // sParam7/nParam7 解析期恒为默认值，但一并纳入以免将来遗漏
    h.string(&c.s_param7);
    h.sep();
    h.num(c.n_param7);
    h.sep();
    h.string(&c.s_op_name);
    h.sep();
    h.string(&c.s_op_h_name);
    h.sep();
    counts.conditions += 1;
}

fn hash_action(h: &mut Hasher, a: &QuestActionInfo, counts: &mut StructureCounts, is_else: bool) {
    h.bytes(if is_else { b"E" } else { b"A" });
    h.sep();
    h.num(a.n_cmd_code);
    h.sep();
    for p in [
        &a.s_param1,
        &a.s_param2,
        &a.s_param3,
        &a.s_param4,
        &a.s_param5,
        &a.s_param6,
    ] {
        h.string(p);
        h.sep();
    }
    for n in [
        a.n_param1, a.n_param2, a.n_param3, a.n_param4, a.n_param5, a.n_param6,
    ] {
        h.num(n);
        h.sep();
    }
    h.string(&a.s_op_name);
    h.sep();
    h.string(&a.s_op_h_name);
    h.sep();
    if is_else {
        counts.else_actions += 1;
    } else {
        counts.actions += 1;
    }
}

/// 商家脚本产物（`boFlag` 路径）——同样纳入摘要，避免「只比 count」的盲区。
#[derive(Debug, Default, Clone)]
pub struct MerchantDigestInput {
    /// `%价格倍率` 行设置的售价倍率（未设置时 C# 属性保持 0）
    pub price_rate: i32,
    pub item_type_list: Vec<i32>,
    pub flags: MerchantFlags,
    pub refill_goods: Vec<Goods>,
}

fn hash_merchant(h: &mut Hasher, m: &MerchantDigestInput) {
    h.bytes(b"M");
    h.sep();
    h.num(m.price_rate);
    h.sep();
    h.num(m.item_type_list.len() as i32);
    h.sep();
    for v in &m.item_type_list {
        h.num(*v);
        h.sep();
    }
    // 标志位图（顺序与 C# harness 读取顺序一致）
    let flags = [
        m.flags.is_buy,
        m.flags.is_sell,
        m.flags.is_make_drug,
        m.flags.is_prices,
        m.flags.is_storage,
        m.flags.is_getback,
        m.flags.is_upgradenow,
        m.flags.is_get_backupgnow,
        m.flags.is_repair,
        m.flags.is_sup_repair,
        m.flags.is_send_msg,
        m.flags.is_use_item_name,
        m.flags.is_offline_msg,
        m.flags.is_yb_deal,
    ];
    for f in flags {
        h.bytes(if f { b"1" } else { b"0" });
    }
    h.sep();
    h.num(m.refill_goods.len() as i32);
    h.sep();
    for g in &m.refill_goods {
        h.string(&g.item_name);
        h.sep();
        h.num(g.count);
        h.sep();
        h.num(g.refill_time);
        h.sep();
    }
}

/// 计算脚本列表 + 商家产物的结构摘要。
pub fn structure_digest_full(
    scripts: &[ScriptInfo],
    merchant: &MerchantDigestInput,
) -> StructureDigest {
    let mut h = Hasher::new();
    let mut counts = StructureCounts::default();
    for s in scripts {
        h.bytes(b"S");
        h.sep();
        h.num(s.quest_count);
        h.sep();
        h.num(i32::from(s.is_quest));
        h.sep();
        counts.scripts += 1;
        for rec in &s.record_list {
            h.bytes(b"R");
            h.sep();
            h.string(&rec.s_label);
            h.sep();
            h.num(i32::from(rec.bo_ext_jmp));
            h.sep();
            counts.records += 1;
            for proc_ in &rec.procedure_list {
                h.bytes(b"P");
                h.sep();
                h.string(&proc_.s_say_msg);
                h.sep();
                h.string(&proc_.s_else_say_msg);
                h.sep();
                counts.procedures += 1;
                for c in &proc_.condition_list {
                    hash_condition(&mut h, c, &mut counts);
                }
                for a in &proc_.action_list {
                    hash_action(&mut h, a, &mut counts, false);
                }
                for a in &proc_.else_action_list {
                    hash_action(&mut h, a, &mut counts, true);
                }
            }
        }
    }
    hash_merchant(&mut h, merchant);
    StructureDigest {
        hash: format!("{:016x}", h.state),
        counts,
    }
}

/// 仅脚本部分（供不需要商家产物的调用点使用）。
pub fn structure_digest(scripts: &[ScriptInfo]) -> StructureDigest {
    structure_digest_full(scripts, &MerchantDigestInput::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SayingProcedure, SayingRecord};

    fn sample() -> Vec<ScriptInfo> {
        let mut s = ScriptInfo::default();
        let mut rec = SayingRecord {
            s_label: "@main".into(),
            procedure_list: vec![SayingProcedure {
                s_say_msg: "你好".into(),
                ..Default::default()
            }],
            bo_ext_jmp: true,
        };
        rec.procedure_list[0]
            .condition_list
            .push(QuestConditionInfo {
                cmd_code: 9,
                s_param1: "祈福项链".into(),
                n_param2: 1,
                ..Default::default()
            });
        s.record_list.push(rec);
        vec![s]
    }

    #[test]
    fn digest_is_deterministic_and_sensitive() {
        let a = structure_digest(&sample());
        let b = structure_digest(&sample());
        assert_eq!(a, b);
        assert_eq!(a.counts.conditions, 1);
        // 改一个参数字节必须变
        let mut s = sample();
        s[0].record_list[0].procedure_list[0].condition_list[0].n_param2 = 2;
        assert_ne!(a.hash, structure_digest(&s).hash);
        // 改 label 必须变（覆盖重名改名序列）
        let mut s = sample();
        s[0].record_list[0].s_label = "@main1".into();
        assert_ne!(a.hash, structure_digest(&s).hash);
        // 字符串长度前缀避免歧义：("ab","c") 与 ("a","bc") 不同
        let mut s1 = sample();
        s1[0].record_list[0].procedure_list[0].s_say_msg = "ab".into();
        s1[0].record_list[0].procedure_list[0].s_else_say_msg = "c".into();
        let mut s2 = sample();
        s2[0].record_list[0].procedure_list[0].s_say_msg = "a".into();
        s2[0].record_list[0].procedure_list[0].s_else_say_msg = "bc".into();
        assert_ne!(structure_digest(&s1).hash, structure_digest(&s2).hash);
    }

    #[test]
    fn merchant_section_is_sensitive() {
        let base = structure_digest(&sample());
        let with_goods = structure_digest_full(
            &sample(),
            &MerchantDigestInput {
                refill_goods: vec![Goods {
                    item_name: "木剑".into(),
                    count: 1,
                    refill_time: 60,
                }],
                ..Default::default()
            },
        );
        assert_ne!(base.hash, with_goods.hash);
        let f = MerchantFlags {
            is_repair: true,
            ..Default::default()
        };
        let with_flag = structure_digest_full(
            &sample(),
            &MerchantDigestInput {
                flags: f,
                ..Default::default()
            },
        );
        assert_ne!(base.hash, with_flag.hash);
    }
}
