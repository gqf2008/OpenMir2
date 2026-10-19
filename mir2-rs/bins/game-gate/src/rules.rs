//! 网关侧的**速度/间隔规则**（照 `src/GameGate/Services/ClientSession.cs`）。
//!
//! 本模块只放**纯函数 + 计时状态**，时钟由调用方传进来（`now_ms`）——这样规则本身可以用
//! 显式时间戳单测（可证伪），而生产路径照 oracle 用真实时钟（`HUtil32.GetTickCount`）。
//! 差分/回放若要按金标准 `ts_ms` 推进，只需在调用方喂虚拟时间（协议梳理 §7.10.2 的方案 ②）。
//!
//! 三条规则（`ClientSession.cs:221-388`）：
//! - `CM_WALK`/`CM_RUN`：`MoveInterval`（+ 累犯时 `PunishMoveInterval`）；
//! - 攻击族（`CM_HIT`…`CM_FIREHIT`）：`AttackInterval`（+ `PunishAttackInterval`），再按
//!   `MaxItemSpeedRate * ItemSpeed` 缩减（`HUtil32._MAX(0, …)`）；
//! - `CM_SPELL`：`TableDef.MaigicDelayTimeList[Tag]`（+ `PunishSpellInterval`），`Tag >= 128` 直接丢，
//!   `MaigicDelayArray[Tag] == false`（武士魔法）不判间隔。
//!
//! **惩罚支**照 oracle 原样（含其 Bug）：
//! - `OverSpeedPunishMethod == DelaySend` 且**延时队列为空** ⇒ 不算"丢弃"，而是把对应 tick
//!   推到"当前 + 惩罚延迟"，本帧照常放行（move/attack）；spell 分支 oracle 只是**计算** `dwDelay`
//!   却从不使用（`ClientSession.cs:371-376`）⇒ 等价实现 = 放行且不刷新计时。
//! - 延时队列非空 ⇒ 把 tick 推到"当前 + 剩余间隔"，并**丢弃本帧**；队列长度 ≥ 2 时 oracle 调
//!   `SendKickMsg(0)`，但该函数的发送代码整段被注释（`:993-1046` 只 `LogService.Debug`）⇒ **不发帧**，
//!   这里也只记日志并丢帧（不关会话）。
//! - **延时队列模型**：`SendDelayMsg` 的**唯一**调用点是登录验证包（`ClientSession.cs:1252`，
//!   `DelayTime=1`）；`TimedService.ProcessDelayMsg`（`TimedService.cs:93-97`，10ms `PeriodicTimer`）
//!   每拍在 `currentTick - ProcessDelayTick > 200` 时 drain。`DelayMsgQueue` 照此实现，
//!   相位确定化的口径见其文档。
//! - 其它 `PunishMethod`（TurnPack/DropPack/NullPack）在这三个分支里**什么也不做** ⇒ 本帧照常放行。
//!
//! **未接线（登记为余量）**：`SessionSpeedRule.ItemSpeed` 由下行 `SM_CHARSTATUSCHANGED`
//! （`ClientSession.cs:955-960`）设置，`SpeedLimit` 由登录时的 `GateShare.PunishList` 设置
//! （`:1102-1106`）；本切片的会话循环尚不解析下行 `GM_DATA` 业务帧，故二者保持 oracle 初值
//! （`0` / `false`）。两条都是本模块的**输入字段**，接线只需赋值，规则本身不变。

/// `TableDef.MaigicNameList`（`src/GameGate/TableDef.cs:151`，128 项，空串 = 该 Tag 无名字）。
///
/// 只用于 `[MagicInterval]` 的按名覆盖（`ConfigManager.cs:156-163`）。
pub const MAGIC_NAMES: [&str; 128] = [
    "",
    "火球术",
    "治愈术",
    "基本剑术",
    "精神力战法",
    "大火球",
    "施毒术",
    "攻杀剑术",
    "抗拒火环",
    "地狱火",
    "疾光电影",
    "雷电术",
    "刺杀剑术",
    "灵魂火符",
    "幽灵盾",
    "神圣战甲术",
    "困魔咒",
    "召唤骷髅",
    "隐身术",
    "集体隐身术",
    "诱惑之光",
    "瞬息移动",
    "火墙",
    "爆裂火焰",
    "地狱雷光",
    "半月弯刀",
    "烈火剑法",
    "野蛮冲撞",
    "心灵启示",
    "群体治疗术",
    "召唤神兽",
    "魔法盾",
    "圣言术",
    "冰咆哮",
    "解毒术",
    "老狮子吼",
    "火焰冰",
    "群体雷电术",
    "群体施毒术",
    "彻地钉",
    "双龙斩",
    "狮子吼",
    "龙影剑法",
    "雷霆剑法",
    "寒冰掌",
    "灭天火",
    "召唤英雄",
    "火龙烈焰",
    "气功波",
    "净化术",
    "无极真气",
    "飓风破",
    "诅咒术",
    "血咒",
    "骷髅咒",
    "",
    "逐日剑法",
    "噬血术",
    "流星火雨",
    "",
    "破魂斩",
    "劈星斩",
    "雷霆一击",
    "噬魂沼泽",
    "末日审判",
    "火龙气焰",
    "开天斩",
    "神秘解读",
    "唯我独尊",
    "",
    "英雄出击",
    "擒龙手",
    "",
    "",
    "",
    "护体神盾",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "追心刺",
    "三绝杀",
    "断岳斩",
    "横扫千军",
    "凤舞祭",
    "惊雷爆",
    "冰天雪地",
    "双龙破",
    "虎啸诀",
    "八卦掌",
    "三焰咒",
    "万箭归宗",
    "旋转风火轮",
    "断空斩",
    "倚天辟地",
    "血魂一击(战)",
    "血魂一击(法)",
    "血魂一击(道)",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
    "",
];

/// `TableDef.MaigicDelayTimeList` 的**默认值**（`TableDef.cs:150`；`MAIGIC_DELAY_TIME_LIST_DEF`）。
///
/// 现网 `[MagicInterval]` 的覆盖值与默认值**逐项相同**（实测见 `config.rs` 的部署夹具测试），
/// 但覆盖机制仍按 oracle 实现。
pub const MAGIC_DELAY_TIME_LIST_DEF: [i64; 128] = [
    60000, 1170, 1150, 1110, 1110, 1170, 1150, 1110, 1140, 1170, 1210, 1210, 1110, 1170, 1150,
    1150, 1160, 1160, 1160, 1160, 1170, 1160, 1230, 1170, 1170, 1110, 1110, 1110, 1150, 1150, 1230,
    1110, 1230, 1170, 1150, 1170, 1170, 1230, 1230, 1230, 1110, 1140, 1110, 1110, 1170, 1170, 1170,
    1170, 1140, 1150, 1140, 1150, 1150, 1150, 1170, 1170, 1110, 1170, 1230, 1300, 200, 200, 200,
    200, 200, 200, 1410, 1710, 1110, 1110, 1610, 1710, 1110, 1110, 100, 100, 100, 100, 100, 100,
    100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100,
    100, 350, 350, 350, 350, 350, 350, 350, 350, 350, 350, 350, 350, 1300, 100, 100, 100, 100, 100,
    100, 100, 100, 100, 100, 100, 100, 100, 100, 100,
];

/// `TableDef.MaigicDelayArray`（`TableDef.cs:17`）：`false` = 该魔法**不走**施法间隔
/// （oracle 注释「过滤武士魔法」）。
pub const MAGIC_DELAY_ARRAY: [bool; 128] = [
    true, true, true, true, true, true, true, false, true, true, true, true, false, true, true,
    true, true, true, true, true, true, true, true, true, true, false, false, false, true, true,
    true, true, true, true, true, false, true, true, true, true, false, false, false, false, true,
    true, true, true, true, true, true, true, true, true, true, false, false, true, true, false,
    false, false, false, false, false, false, false, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, true, false, false, true, true, false, false, false,
    false, false, false, false, false, false, false,
];

/// `TableDef.MaigicAttackArray`（`TableDef.cs:68`）：`true` = 该魔法通过后只补偿
/// `SpellNextMove/AttackCompensate`；`false` 再多加 80ms（`ClientSession.cs:351-358`）。
pub const MAGIC_ATTACK_ARRAY: [bool; 128] = [
    false, true, false, false, false, true, true, false, false, true, true, true, false, true,
    false, false, false, false, false, false, false, false, true, true, true, false, false, false,
    false, false, false, false, true, true, false, false, true, true, true, true, false, true,
    false, false, true, true, false, true, false, false, false, true, true, true, true, false,
    false, true, true, false, false, false, false, false, false, false, false, false, false, false,
    false, true, true, true, true, true, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true,
    true, true, true, true, true, true, true, true, true, true, true,
];

/// `GateShare.PunishMethod`（`GateShare.cs:198-214`）：`[Method] OverSpeedPunishMethod` 的枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PunishMethod {
    /// `0`：转换封包（本切片的三条规则里不处理）。
    TurnPack,
    /// `1`：丢弃封包（本切片的三条规则里**不做任何事**——照 oracle）。
    DropPack,
    /// `2`：无效封包（同上）。
    NullPack,
    /// `3`：延时处理（现网配置值；见模块文档）。
    DelaySend,
}

impl PunishMethod {
    /// `(PunishMethod)ReadWriteInteger(...)`（`ConfigManager.cs:123`）：只有 `3` 有行为差异。
    #[must_use]
    pub fn from_raw(raw: i64) -> Self {
        match raw {
            1 => Self::DropPack,
            2 => Self::NullPack,
            3 => Self::DelaySend,
            _ => Self::TurnPack,
        }
    }
}

/// 网关侧全部"按帧"规则的**配置打包**（避免给会话函数塞一长串参数）。
///
/// `chat_*` 是 `CM_SAY` 的间隔门（`ClientSession.cs:485-497`）；`speed` 是 move/attack/spell
/// 三条规则（`ClientSession.cs:221-388`）。
#[derive(Debug, Clone)]
pub struct GateRules {
    /// `Config.IsChatInterval`（`[Switch] ChatInterval`）。
    pub chat_interval_enabled: bool,
    /// `Config.ChatInterval`（`[Integer] ChatInterval`）。
    pub chat_interval_ms: i64,
    /// move/attack/spell 的间隔与惩罚配置（`GateConfig` 对应键）。
    pub speed: SpeedRules,
}

/// 三条速度规则的配置（`GateConfig` 字段的镜像；缺省值 = `GateConfig()` 构造函数，
/// `Conf/GateConfig.cs:229-300`）。
#[derive(Debug, Clone)]
// oracle 的 `GateConfig` 本身就是一组 bool 开关，拆成枚举反而偏离逐键对应。
#[allow(clippy::struct_excessive_bools)]
pub struct SpeedRules {
    /// `[Switch] MoveInterval`（`Config.IsMoveInterval`）。
    pub move_interval_enabled: bool,
    /// `[Switch] AttackInterval`（`Config.IsAttackInterval`）。
    pub attack_interval_enabled: bool,
    /// `[Switch] SpellInterval`（`Config.IsSpellInterval`）。
    pub spell_interval_enabled: bool,
    /// `[Integer] MoveInterval`（缺省 570；现网 366）。
    pub move_interval: i64,
    /// `[Integer] AttackInterval`（缺省 900；现网 500）。
    pub attack_interval: i64,
    /// `[Integer] PunishMoveInterval`（缺省/现网 150）。
    pub punish_move_interval: i64,
    /// `[Integer] PunishAttackInterval`（缺省/现网 150）。
    pub punish_attack_interval: i64,
    /// `[Integer] PunishSpellInterval`（缺省/现网 150）。
    pub punish_spell_interval: i64,
    /// `[Integer] PunishBaseInterval`（缺省/现网 20）。
    pub punish_base_interval: i64,
    /// `[Float] PunishIntervalRate`（缺省/现网 1.0）。
    pub punish_interval_rate: f64,
    /// `[Method] OverSpeedPunishMethod`（缺省/现网 `DelaySend`）。
    pub punish_method: PunishMethod,
    /// `[Integer] MoveNextSpellCompensate`（缺省 100）。
    pub move_next_spell_compensate: i64,
    /// `[Integer] MoveNextAttackCompensate`（缺省 250）。
    pub move_next_attack_compensate: i64,
    /// `[Integer] AttackNextMoveCompensate`（缺省 200）。
    pub attack_next_move_compensate: i64,
    /// `[Integer] AttackNextSpellCompensate`（缺省 200）。
    pub attack_next_spell_compensate: i64,
    /// `[Integer] SpellNextMoveCompensate`（缺省 200）。
    pub spell_next_move_compensate: i64,
    /// `[Integer] SpellNextAttackCompensate`（缺省 200）。
    pub spell_next_attack_compensate: i64,
    /// `[Switch] ItemSpeedCompensate`（缺省 false）。
    pub item_speed_compensate: bool,
    /// `[Integer] MaxItemSpeedRate`（缺省/现网 60）：`ItemSpeed` 每点抵多少毫秒攻击间隔。
    pub max_item_speed_rate: i64,
    /// `TableDef.MaigicDelayTimeList`（默认值 + `[MagicInterval]` 覆盖）。
    pub magic_delay_time_list: [i64; 128],
}

impl SpeedRules {
    /// `GateConfig()` 构造函数里的缺省值（`Conf/GateConfig.cs:229-300`）。
    #[must_use]
    pub fn oracle_defaults() -> Self {
        Self {
            move_interval_enabled: true,
            attack_interval_enabled: true,
            spell_interval_enabled: true,
            move_interval: 570,
            attack_interval: 900,
            punish_move_interval: 150,
            punish_attack_interval: 150,
            punish_spell_interval: 150,
            punish_base_interval: 20,
            punish_interval_rate: 1.0,
            punish_method: PunishMethod::DelaySend,
            move_next_spell_compensate: 100,
            move_next_attack_compensate: 250,
            attack_next_move_compensate: 200,
            attack_next_spell_compensate: 200,
            spell_next_move_compensate: 200,
            spell_next_attack_compensate: 200,
            item_speed_compensate: false,
            max_item_speed_rate: 60,
            magic_delay_time_list: MAGIC_DELAY_TIME_LIST_DEF,
        }
    }
}

/// 一帧的处置结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// 放行（照常转发）。
    Forward,
    /// 丢弃本帧（`return`，不转发、不关会话）。
    Drop,
    /// 丢弃本帧，并照 oracle `SendKickMsg(0)` **只记一条日志**（不发帧、不关会话）。
    DropKick,
}

/// `ClientSession.MsgList` + `TimedService.ProcessDelayMsg` 的最小等价模型。
///
/// oracle 的延时队列只被 `SendDelayMsg` 写入，而该方法**唯一**的调用点是登录验证包
/// （`ClientSession.cs:1252`，`DelayTime = 1`）⇒ 生产路径上队列长度只可能是 0 或 1
/// （`count >= 2` 只在构造级可达；`SendKickMsg` 支仍照原样保留）。
///
/// 时序照 `TimedService`：`PeriodicTimer(10ms)` 每拍检查 `currentTick - ProcessDelayTick > 200`，
/// 成立时把 `ProcessDelayTick` 推到当前拍并 drain（`TimedService.cs:93-97`）；drain 删除
/// `DelayTime == 0 || now >= DelayTime` 的条目（`ClientSession.cs:836-846`）。
///
/// **相位确定化（有意偏离，登记）**：oracle 的 `ProcessDelayTick` 是**全网关共享**的服务启动
/// 时刻，每个会话看到的 drain 相位取决于登录落在哪个 10ms 槽（登录条目存活 0–210ms，
/// 相位不可判）。这里把计时基线挂到**会话创建时刻**（等价于「登录紧接一次 drain」），
/// 条目固定存活到创建后第一个 `> 200ms` 的 10ms 拍（210ms）⇒ 取 oracle 的**上界**，
/// 使判据①「登录后 0–210ms 内 move ⇒ 丢帧」可确定地复现。
#[derive(Debug, Clone)]
pub struct DelayMsgQueue {
    /// `TimedService.ProcessDelayTick`（本会话等价）。
    process_delay_tick: i64,
    /// `PeriodicTimer(10ms)` 的上一次触发时刻。
    timer_tick: i64,
    /// `MsgList` 各条目的 `DelayTime`（`0` = 无延迟）。
    delay_times: Vec<i64>,
}

impl DelayMsgQueue {
    /// 新会话：计时基线 = 会话创建时刻。
    #[must_use]
    pub fn new(now_ms: i64) -> Self {
        Self {
            process_delay_tick: now_ms,
            timer_tick: now_ms,
            delay_times: Vec::new(),
        }
    }

    /// `SendDelayMsg(..., delayTime)`（`ClientSession.cs:860-878`）：入队 `DelayTime = now + delayTime`。
    pub fn send_delay_msg(&mut self, now_ms: i64, delay_time: i64) {
        self.delay_times.push(now_ms + delay_time);
    }

    /// `GetDelayMsgCount()`（`ClientSession.cs:823-826`）。
    #[must_use]
    pub fn count(&self) -> usize {
        self.delay_times.len()
    }

    /// 把 10ms 定时器推进到 `now_ms`，按 `ProcessDelayMsg` drain。
    ///
    /// 规则帧入口调用即可：drain 只取决于墙上时刻，惰性推进与 oracle 的后台线程推进对
    /// 「帧到达时看到的 count」等价。
    pub fn tick(&mut self, now_ms: i64) {
        while now_ms - self.timer_tick >= 10 {
            self.timer_tick += 10;
            if self.timer_tick - self.process_delay_tick > 200 {
                self.process_delay_tick = self.timer_tick;
                self.drain(self.timer_tick);
            }
        }
    }

    /// `GetDelayMessage`（`ClientSession.cs:836-846`）：保留 `DelayTime != 0 && now < DelayTime`。
    fn drain(&mut self, now_ms: i64) {
        self.delay_times
            .retain(|&delay_time| delay_time != 0 && now_ms < delay_time);
    }
}

/// `SessionSpeedRule` 里本切片用到的计时/动作状态（`Services/SessionSpeedRule.cs`）。
///
/// 构造时把三个 tick 都设为**会话创建时刻**，照 `SessionSpeedRule()` 构造函数
/// （`SessionSpeedRule.cs:118-146` 的 `MoveTick/AttackTick/SpellTick = dwCurrentTick`）。
#[derive(Debug, Clone)]
pub struct SpeedState {
    /// `SessionSpeedRule.MoveTick`。
    pub move_tick: i64,
    /// `SessionSpeedRule.AttackTick`。
    pub attack_tick: i64,
    /// `SessionSpeedRule.SpellTick`。
    pub spell_tick: i64,
    /// `SessionSpeedRule.ItemSpeed`（下行 `SM_CHARSTATUSCHANGED` 设置；本切片恒为初值 0）。
    pub item_speed: i64,
    /// `SessionSpeedRule.SpeedLimit`（登录时命中 `PunishList` 才置 true；本切片恒为初值 false）。
    pub speed_limit: bool,
    /// `ClientSession.LastDirection`（初值 -1，`ClientSession.cs:60`）。
    ///
    /// 三条规则通过时都刷新它（oracle 供 `CM_TURN` 用）。本切片未实现 `CM_TURN`，故只写不读；
    /// 保留字段是为了把「通过后刷新」的完整状态照搬，后续接 `CM_TURN` 不需改规则。
    #[allow(dead_code)]
    pub last_direction: i32,
    /// `ClientSession.MsgList` + `TimedService.ProcessDelayMsg` 的等价模型
    /// （`GetDelayMsgCount()` 的唯一来源，`ClientSession.cs:823-826`）。
    ///
    /// 登录包发送时入队 `DelayTime = 1`（`:1252` 的 `SendLoginMessage`）；生产路径上
    /// 队列长度只可能是 0 或 1（唯一入队点），`count >= 2` 只在构造级可达。
    pub delays: DelayMsgQueue,
}

impl SpeedState {
    /// 新会话：三个 tick = `now_ms`（照 `SessionSpeedRule` 构造函数）。
    #[must_use]
    pub fn new(now_ms: i64) -> Self {
        Self {
            move_tick: now_ms,
            attack_tick: now_ms,
            spell_tick: now_ms,
            item_speed: 0,
            speed_limit: false,
            last_direction: -1,
            delays: DelayMsgQueue::new(now_ms),
        }
    }

    /// `CM_WALK`/`CM_RUN`（`ClientSession.cs:221-269`）。
    ///
    /// `direction` = `clientMessage.Tag`；oracle 通过时记进 `LastDirection`。
    #[must_use]
    pub fn on_move(&mut self, now_ms: i64, direction: u16, cfg: &SpeedRules) -> Verdict {
        if !cfg.move_interval_enabled {
            return Verdict::Forward;
        }
        self.delays.tick(now_ms);
        let delay_msg_count = self.delays.count();
        let need = cfg.move_interval
            + if self.speed_limit {
                cfg.punish_move_interval
            } else {
                0
            };
        let elapsed = now_ms - self.move_tick;
        if elapsed >= need {
            self.move_tick = now_ms;
            self.spell_tick = now_ms - cfg.move_next_spell_compensate;
            if self.attack_tick < now_ms - cfg.move_next_attack_compensate {
                self.attack_tick = now_ms - cfg.move_next_attack_compensate;
            }
            self.last_direction = i32::from(direction);
            return Verdict::Forward;
        }
        if cfg.punish_method != PunishMethod::DelaySend {
            // oracle：非 DelaySend 时不进任何分支 ⇒ 本帧照常放行，计时也不动。
            return Verdict::Forward;
        }
        if delay_msg_count == 0 {
            // `:249-252`：把下一次允许时间推到 `now + dwDelay`，本帧放行。
            self.move_tick = now_ms + punish_delay(cfg, need - elapsed);
            Verdict::Forward
        } else {
            // `:254-262`：队列非空 ⇒ 推到「当前 + 剩余间隔」，丢帧；≥2 时 `SendKickMsg(0)`。
            self.move_tick = now_ms + (need - elapsed);
            if delay_msg_count >= 2 {
                Verdict::DropKick
            } else {
                Verdict::Drop
            }
        }
    }

    /// 攻击族 `CM_HIT`/`CM_HEAVYHIT`/…/`CM_FIREHIT`（`ClientSession.cs:270-330`）。
    #[must_use]
    pub fn on_attack(&mut self, now_ms: i64, direction: u16, cfg: &SpeedRules) -> Verdict {
        if !cfg.attack_interval_enabled {
            return Verdict::Forward;
        }
        let need = cfg.attack_interval
            + if self.speed_limit {
                cfg.punish_attack_interval
            } else {
                0
            };
        self.delays.tick(now_ms);
        let delay_msg_count = self.delays.count();
        // `HUtil32._MAX(0, (nAttackInterval - Config.MaxItemSpeedRate * gameSpeed.ItemSpeed))`
        let fixed_need = (need - cfg.max_item_speed_rate * self.item_speed).max(0);
        let elapsed = now_ms - self.attack_tick;
        if elapsed >= fixed_need {
            self.attack_tick = now_ms;
            let extra = if cfg.item_speed_compensate {
                cfg.max_item_speed_rate * self.item_speed
            } else {
                0
            };
            self.move_tick = now_ms - (cfg.attack_next_move_compensate + extra);
            self.spell_tick = now_ms - (cfg.attack_next_spell_compensate + extra);
            self.last_direction = i32::from(direction);
            return Verdict::Forward;
        }
        if cfg.punish_method != PunishMethod::DelaySend {
            return Verdict::Forward;
        }
        if delay_msg_count == 0 {
            self.attack_tick = now_ms + punish_delay(cfg, fixed_need - elapsed);
            Verdict::Forward
        } else {
            self.attack_tick = now_ms + (fixed_need - elapsed);
            if delay_msg_count >= 2 {
                Verdict::DropKick
            } else {
                Verdict::Drop
            }
        }
    }

    /// `CM_SPELL`（`ClientSession.cs:331-388`）。
    ///
    /// 两个**照搬的 oracle 行为**：
    /// - `Tag >= 128` ⇒ **丢弃**（`:333-336`，在取表之前）；
    /// - `MaigicDelayArray[Tag] == false` ⇒ **不判间隔、直接放行**（`:335`）。
    ///
    /// 惩罚支的 `DelaySend` + 空队列分支 oracle **计算 `dwDelay` 后从不使用**（`:371-376`）：
    /// 不刷新 `SpellTick`、也不施加延迟 ⇒ 本帧放行（这里不计算，避免引入不存在的效果）。
    #[must_use]
    pub fn on_spell(&mut self, now_ms: i64, tag: u16, cfg: &SpeedRules) -> Verdict {
        if !cfg.spell_interval_enabled {
            return Verdict::Forward;
        }
        if tag >= 128 {
            return Verdict::Drop;
        }
        let idx = usize::from(tag);
        if !MAGIC_DELAY_ARRAY[idx] {
            return Verdict::Forward;
        }
        self.delays.tick(now_ms);
        let delay_msg_count = self.delays.count();
        let need = cfg.magic_delay_time_list[idx]
            + if self.speed_limit {
                cfg.punish_spell_interval
            } else {
                0
            };
        let elapsed = now_ms - self.spell_tick;
        if elapsed >= need {
            let (next_move, next_attack) = if MAGIC_ATTACK_ARRAY[idx] {
                (
                    cfg.spell_next_move_compensate,
                    cfg.spell_next_attack_compensate,
                )
            } else {
                (
                    cfg.spell_next_move_compensate + 80,
                    cfg.spell_next_attack_compensate + 80,
                )
            };
            self.spell_tick = now_ms;
            self.move_tick = now_ms - next_move;
            self.attack_tick = now_ms - next_attack;
            self.last_direction = i32::from(tag);
            return Verdict::Forward;
        }
        if cfg.punish_method != PunishMethod::DelaySend {
            return Verdict::Forward;
        }
        if delay_msg_count == 0 {
            // oracle 在这里算 dwDelay 但不使用（Bug，连同 Bug 一起等价）：放行、不刷新。
            Verdict::Forward
        } else if delay_msg_count >= 2 {
            Verdict::DropKick
        } else {
            Verdict::Drop
        }
    }
}

/// `PunishBaseInterval + Math.Round(剩余 * PunishIntervalRate)`。
///
/// `Math.Round(double)` 默认是**银行家舍入**（`MidpointRounding.ToEven`），
/// 故用 `f64::round_ties_even` 而不是 `round`（后者 .5 远离零）。
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn punish_delay(cfg: &SpeedRules, remaining_ms: i64) -> i64 {
    cfg.punish_base_interval
        + (remaining_ms as f64 * cfg.punish_interval_rate).round_ties_even() as i64
}

/// `CM_SAY` 的间隔计时（`SessionSpeedRule.SayMsgTick` 的对应物）。
///
/// 逐字语义：`msgContent`（= 解码后 `decodeBuff[12..]`）**以 `@` 开头**（`_commandFlagSpan`，
/// `ClientSession.cs:92`）⇒ **不判间隔**；否则距上次 say 不足 `ChatInterval` ⇒ **丢帧**（`return`），
/// 通过则把 `SayMsgTick` 刷新为当前时间。
#[derive(Debug, Clone, Copy, Default)]
pub struct ChatThrottle {
    /// `SessionSpeedRule.SayMsgTick`（0 = 从未说过 ⇒ 第一条恒通过）。
    pub say_msg_tick: i64,
}

impl ChatThrottle {
    /// `true` = 允许转发；`false` = 按 oracle 丢掉这一帧。
    ///
    /// # Panics
    /// 不会 panic。
    #[must_use]
    pub fn allows(&mut self, now_ms: i64, content: &[u8], interval_ms: i64) -> bool {
        if content.starts_with(b"@") {
            return true; // 命令不走间隔（oracle 的 `!msgContent.StartsWith("@")` 外）
        }
        if now_ms - self.say_msg_tick < interval_ms {
            return false; // :492-495 —— 直接 return，不转发
        }
        self.say_msg_tick = now_ms;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 800ms 间隔（现网 `ChatInterval=800`）：第一条通过、紧接着第二条被丢、跨过间隔后又通过。
    #[test]
    fn say_interval_drops_the_second_message_too_soon() {
        let mut t = ChatThrottle::default();
        assert!(t.allows(1_000, b"hello", 800), "第一条（上次=0）必须通过");
        assert!(!t.allows(1_500, b"again", 800), "500ms 后第二条必须被丢");
        assert!(!t.allows(1_799, b"again", 800), "边界：799ms 仍要丢");
        assert!(
            t.allows(1_800, b"again", 800),
            "恰好 800ms 必须通过（>= interval）"
        );
        // 被丢的那两条**不刷新**计时：所以 1_800 这次通过后，1_801 又被丢
        assert!(!t.allows(1_801, b"x", 800));
    }

    /// `@` 开头的命令内容不走间隔（`ClientSession.cs:489` 的 `!StartsWith("@")` 之外）。
    #[test]
    fn at_prefixed_content_skips_interval_and_does_not_refresh_tick() {
        let mut t = ChatThrottle::default();
        assert!(t.allows(1_000, b"@move 1 2", 800));
        assert!(
            t.allows(1_001, b"@move 1 2", 800),
            "@ 命令不判间隔，连续两条都通过"
        );
        assert_eq!(t.say_msg_tick, 0, "走 @ 分支不刷新 SayMsgTick");
        assert!(t.allows(1_002, b"plain", 800), "普通内容仍按 last=0 通过");
    }

    /// 关掉开关（`[Switch] ChatInterval=0`）时调用方不该调本函数（见 main）；这里是"interval=0"的边界。
    #[test]
    fn zero_interval_never_drops() {
        let mut t = ChatThrottle::default();
        assert!(t.allows(100, b"a", 0));
        assert!(
            t.allows(100, b"b", 0),
            "interval=0 ⇒ now-last=0 不小于 0 ⇒ 通过"
        );
    }

    /// 现网配置（`E:/MirServer/RunGate/config.conf` 的可观测值；其余同缺省）。
    fn deployed() -> SpeedRules {
        SpeedRules {
            move_interval: 366,
            attack_interval: 500,
            ..SpeedRules::oracle_defaults()
        }
    }

    /// 构造「队列非空」：入队一条 `DelayTime = now + 1`（oracle 生产路径唯一的入队形态，
    /// `ClientSession.cs:1252`）。
    fn enqueue_delay(s: &mut SpeedState, now_ms: i64) {
        s.delays.send_delay_msg(now_ms, 1);
    }

    // ---------------- move ----------------

    /// 判据①②③：首帧（从会话创建的 tick 起算）通过；恰好等于间隔通过（`>=` 而非 `>`）；
    /// 通过后刷新 MoveTick/SpellTick/AttackTick。
    ///
    /// 牙①：把 `>=` 改 `>` ⇒ `on_move(366)` 变 Drop。
    /// 牙②：忘刷新 MoveTick ⇒ 367ms 那一帧（queue=1）不会被丢。
    #[test]
    fn move_passes_at_exact_interval_and_refreshes_ticks() {
        let c = deployed();
        let mut s = SpeedState::new(0);
        assert_eq!(
            s.on_move(366, 1, &c),
            Verdict::Forward,
            "恰好 366ms 必须通过"
        );
        assert_eq!(s.move_tick, 366);
        assert_eq!(s.spell_tick, 366 - 100, "MoveNextSpellCompensate=100");
        assert_eq!(s.attack_tick, 366 - 250, "MoveNextAttackCompensate=250");
        assert_eq!(s.last_direction, 1);
        // 通过后立即再来一帧：queue=1 ⇒ 丢（oracle 的 DelaySend 非空队列分支）。
        enqueue_delay(&mut s, 367);
        assert_eq!(s.on_move(367, 2, &c), Verdict::Drop);
    }

    /// 判据②：间隔内被丢（queue 非空）；边界前 1ms 仍丢。
    #[test]
    fn move_inside_interval_is_dropped() {
        let c = deployed();
        let mut s = SpeedState::new(0);
        enqueue_delay(&mut s, 365);
        assert_eq!(s.on_move(365, 0, &c), Verdict::Drop);
        assert_eq!(
            s.move_tick,
            365 + (366 - 365),
            "非空队列分支：now + 剩余间隔"
        );
    }

    /// 判据⑤：DelaySend + **空队列** ⇒ 早发帧照 oracle 值**推迟下一次允许时间**，本帧放行。
    ///
    /// 牙③：惩罚延迟算反（`base + elapsed` / `now - dwDelay`）⇒ `move_tick` 不等于 752。
    #[test]
    fn move_punish_delay_matches_oracle_formula() {
        let c = deployed();
        let mut s = SpeedState::new(0);
        assert_eq!(s.on_move(366, 0, &c), Verdict::Forward);
        // now=500：elapsed=134，剩余=232；dwDelay = 20 + round(232*1.0) = 252。
        assert_eq!(s.on_move(500, 0, &c), Verdict::Forward, "空队列 ⇒ 本帧放行");
        assert_eq!(s.move_tick, 752, "500 + 20 + (366-134)");
        // 752 之前（含 751）仍在惩罚窗口内；queue 非空 ⇒ 丢。
        enqueue_delay(&mut s, 751);
        assert_eq!(s.on_move(751, 0, &c), Verdict::Drop);
        assert_eq!(s.move_tick, 751 + (366 - (-1)));
    }

    /// 判据⑤：队列 ≥2 时 oracle 调 `SendKickMsg(0)`——只记日志、**不关会话**，
    /// 对应 `DropKick`（调用方仍按丢帧处理）。
    #[test]
    fn move_drop_kick_only_above_queue_two() {
        let c = deployed();
        let mut s = SpeedState::new(0);
        s.move_tick = 1_000;
        enqueue_delay(&mut s, 1_100);
        enqueue_delay(&mut s, 1_100);
        assert_eq!(s.on_move(1_100, 0, &c), Verdict::DropKick);
    }

    /// `Math.Round` 是银行家舍入（ToEven）：0.5→0、1.5→2。
    ///
    /// 牙③（细化）：用 `round()`（远离零）⇒ 第一条断言得 121 而不是 120。
    #[test]
    fn punish_delay_uses_bankers_rounding() {
        let mut c = deployed();
        c.punish_interval_rate = 0.5;

        let mut half_down = SpeedState::new(0);
        half_down.move_tick = 100 - 365; // 剩余 1ms
        assert_eq!(half_down.on_move(100, 0, &c), Verdict::Forward);
        assert_eq!(half_down.move_tick, 120, "1*0.5 舍到 0 ⇒ 20+0");

        let mut half_up = SpeedState::new(0);
        half_up.move_tick = 100 - 363; // 剩余 3ms
        assert_eq!(half_up.on_move(100, 0, &c), Verdict::Forward);
        assert_eq!(half_up.move_tick, 122, "3*0.5=1.5 舍到 2 ⇒ 20+2");
    }

    // ---------------- attack ----------------

    /// 判据③：攻击间隔；通过后按 `AttackNextMove/SpellCompensate` 反向补偿。
    #[test]
    fn attack_passes_at_exact_interval_and_compensates() {
        let c = deployed();
        let mut s = SpeedState::new(0);
        assert_eq!(s.on_attack(500, 3, &c), Verdict::Forward, "恰好 500ms 通过");
        assert_eq!(s.attack_tick, 500);
        assert_eq!(s.move_tick, 300, "500 - AttackNextMoveCompensate(200)");
        assert_eq!(s.spell_tick, 300, "500 - AttackNextSpellCompensate(200)");
        assert_eq!(s.last_direction, 3);
        enqueue_delay(&mut s, 501);
        assert_eq!(s.on_attack(501, 0, &c), Verdict::Drop);
    }

    /// 判据⑤：`MaxItemSpeedRate * ItemSpeed` 缩短攻击间隔（`_MAX(0, …)`）。
    ///
    /// 牙④：漏乘 ItemSpeed / 用 move 表 ⇒ 140ms 处不会通过。
    #[test]
    fn attack_interval_is_reduced_by_item_speed() {
        let c = deployed();
        let mut s = SpeedState::new(0);
        s.item_speed = 6; // 500 - 60*6 = 140
        assert_eq!(
            s.on_attack(139, 0, &c),
            Verdict::Forward,
            "空队列 ⇒ 惩罚分支放行"
        );
        assert_eq!(s.attack_tick, 139 + 20 + (140 - 139), "20 + round(1*1.0)");
        let mut ok = SpeedState::new(0);
        ok.item_speed = 6;
        assert_eq!(ok.on_attack(140, 0, &c), Verdict::Forward);
        assert_eq!(ok.attack_tick, 140);
    }

    /// `IsItemSpeedCompensate=1` 时，攻击后的补偿也减去 `MaxItemSpeedRate*ItemSpeed`。
    #[test]
    fn item_speed_compensate_applies_to_followup_actions() {
        let mut c = deployed();
        c.item_speed_compensate = true;
        let mut s = SpeedState::new(0);
        s.item_speed = 6;
        assert_eq!(s.on_attack(140, 0, &c), Verdict::Forward);
        assert_eq!(s.move_tick, 140 - (200 + 360));
        assert_eq!(s.spell_tick, 140 - (200 + 360));
    }

    /// 判据⑤：攻击的惩罚延迟 = `PunishBaseInterval + round(剩余 * rate)`。
    #[test]
    fn attack_punish_delay_matches_oracle_formula() {
        let c = deployed();
        let mut s = SpeedState::new(0);
        s.attack_tick = 1_000;
        assert_eq!(s.on_attack(1_100, 0, &c), Verdict::Forward);
        assert_eq!(s.attack_tick, 1_520, "1100 + 20 + (500-100)");
    }

    // ---------------- spell ----------------

    /// 判据⑥：按 `Tag` 取表；`MaigicAttackArray` 决定补偿是否 +80。
    ///
    /// 牙④：按错 Tag 取表 ⇒ 500ms 处 tag1 不会通过、或 tag3 会通过。
    #[test]
    fn spell_uses_tag_indexed_table_and_attack_array() {
        let mut c = deployed();
        c.magic_delay_time_list[1] = 500; // 火球术
        let mut s = SpeedState::new(0);
        assert_eq!(s.on_spell(499, 1, &c), Verdict::Forward, "空队列 ⇒ 放行");
        let mut ok = SpeedState::new(0);
        assert_eq!(
            ok.on_spell(500, 1, &c),
            Verdict::Forward,
            "tag1 的表值 = 500"
        );
        assert_eq!(ok.move_tick, 500 - 200, "MaigicAttackArray[1]=true");
        assert_eq!(ok.attack_tick, 500 - 200);
        // 同一配置下 tag3（基本剑术）仍是 1110；queue=1 时 500ms 必须丢。
        let mut other = SpeedState::new(0);
        enqueue_delay(&mut other, 500);
        assert_eq!(other.on_spell(500, 3, &c), Verdict::Drop);
        assert_eq!(
            other.on_spell(1_110, 3, &c),
            Verdict::Forward,
            "spell 丢帧不刷新 SpellTick ⇒ 1110 恰好跨过"
        );
        let mut fresh = SpeedState::new(0);
        assert_eq!(fresh.on_spell(1_110, 3, &c), Verdict::Forward);
        assert_eq!(fresh.move_tick, 1_110 - (200 + 80), "非攻击魔法 +80");
        assert_eq!(fresh.attack_tick, 1_110 - (200 + 80));
    }

    /// 判据⑥：越界 Tag（`>= 128`）**直接丢**，且不动计时（`:333-336`）。
    #[test]
    fn spell_out_of_range_tag_is_dropped_without_touch() {
        let c = deployed();
        let mut s = SpeedState::new(0);
        assert_eq!(s.on_spell(100_000, 128, &c), Verdict::Drop);
        assert_eq!(s.on_spell(100_000, 4_000, &c), Verdict::Drop);
        assert_eq!(s.spell_tick, 0);
        assert_eq!(s.last_direction, -1);
    }

    /// `MaigicDelayArray[Tag] == false`（武士魔法，如 tag 7 攻杀剑术）⇒ 不判间隔、不刷新。
    ///
    /// 牙④（另一面）：若忽略该数组、一律按表判，第二帧（0ms）会被惩罚/丢弃。
    #[test]
    fn spell_skips_interval_for_warrior_magics() {
        let c = deployed();
        let mut s = SpeedState::new(0);
        enqueue_delay(&mut s, 0);
        assert_eq!(s.on_spell(0, 7, &c), Verdict::Forward);
        assert_eq!(s.on_spell(0, 7, &c), Verdict::Forward, "连续两帧都不判");
        assert_eq!(s.spell_tick, 0);
    }

    /// 判据⑤：DelaySend + 空队列时 oracle **算了 dwDelay 却不使用**（Bug 照搬）：
    /// 放行、**不刷新 SpellTick**。
    ///
    /// 牙（bug 修正）：若"顺手修好"（tick = now + dwDelay）⇒ 1170ms 处不再通过。
    #[test]
    fn spell_early_frame_forwards_without_refresh_or_delay() {
        let c = deployed();
        let mut s = SpeedState::new(0);
        assert_eq!(s.on_spell(100, 1, &c), Verdict::Forward);
        assert_eq!(s.spell_tick, 0, "oracle 此处不刷新（dwDelay 未使用）");
        assert_eq!(s.on_spell(1_170, 1, &c), Verdict::Forward, "tick 仍为 0");
    }

    /// 判据⑤：队列非空 ⇒ spell 丢帧；≥2 时 DropKick。
    #[test]
    fn spell_dropped_when_delay_queue_nonempty() {
        let c = deployed();
        let mut s = SpeedState::new(0);
        enqueue_delay(&mut s, 1_169);
        assert_eq!(s.on_spell(1_169, 1, &c), Verdict::Drop);
        assert_eq!(s.spell_tick, 0);
        enqueue_delay(&mut s, 1_169);
        assert_eq!(s.on_spell(1_169, 1, &c), Verdict::DropKick);
    }

    // ---------------- 交叉 / 开关 ----------------

    /// 牙⑤：三条规则必须**各自**用 MoveTick/AttackTick/SpellTick，不能共用一个计时。
    ///
    /// 攻击通过会反向补偿 MoveTick/SpellTick（500→300）；共享计时的话 move@666 的 elapsed
    /// 只有 166、spell@1470 只有 970。**队列必须非空**才咬得住：空队列走 DelaySend 惩罚支
    /// 会无条件放行（旧用例因此对「on_move 改用 attack_tick」仍是绿的）。
    #[test]
    fn three_rules_keep_separate_timers() {
        let c = deployed();
        let mut s = SpeedState::new(0);
        assert_eq!(s.on_attack(500, 0, &c), Verdict::Forward);
        assert_eq!(s.move_tick, 300);
        assert_eq!(s.spell_tick, 300);
        enqueue_delay(&mut s, 666);
        assert_eq!(s.on_move(666, 1, &c), Verdict::Forward, "666-300 == 366");
        assert_eq!(s.move_tick, 666);

        // spell 同理：tag1 的表值 1170；用 SpellTick(300) 恰好通过，用 AttackTick(500) 会丢。
        let mut sp = SpeedState::new(0);
        assert_eq!(sp.on_attack(500, 0, &c), Verdict::Forward);
        enqueue_delay(&mut sp, 1_470);
        assert_eq!(
            sp.on_spell(1_470, 1, &c),
            Verdict::Forward,
            "1470-300 == 1170"
        );
        assert_eq!(sp.spell_tick, 1_470);
    }

    /// F1 判据①/②：登录包入队（`SendDelayMsg(..., 1)`）后，10ms 拍在
    /// `tick - ProcessDelayTick > 200` 时 drain ⇒ 条目存活到创建后第 210ms 拍。
    #[test]
    fn login_delay_message_lives_until_first_210ms_tick() {
        let mut q = DelayMsgQueue::new(0);
        assert_eq!(q.count(), 0);
        q.send_delay_msg(0, 1); // `SendLoginMessage` → `SendDelayMsg(..., 1)`
        assert_eq!(q.count(), 1);
        q.tick(209);
        assert_eq!(q.count(), 1, "200ms 门未过（10ms 拍：200 不 > 200）");
        q.tick(210);
        assert_eq!(q.count(), 0, "210ms 拍 drain（DelayTime=1 已到期）");
    }

    /// F1 判据③登记：`count >= 2` 在 oracle 生产路径**不可达**——唯一入队点是登录包，
    /// 且 `SendDelayMsg` 没有其它调用者；`DropKick` 支只做构造级覆盖。
    ///
    /// 本用例把「生产模型最多 1 条」固化为断言：入队一次后即使时间推进也不增加。
    #[test]
    fn production_delay_queue_never_exceeds_one() {
        let mut q = DelayMsgQueue::new(0);
        q.send_delay_msg(0, 1);
        for t in 0..=500 {
            q.tick(t);
            assert!(q.count() <= 1, "生产路径只有登录包一个入队点");
        }
    }

    /// `SpeedLimit`（累犯）把 `Punish*Interval` 加到三条规则的间隔上。
    #[test]
    fn speed_limit_adds_punish_intervals() {
        let c = deployed();
        let mut m = SpeedState::new(0);
        m.speed_limit = true;
        enqueue_delay(&mut m, 515);
        assert_eq!(m.on_move(515, 0, &c), Verdict::Drop, "366+150=516");
        let mut m2 = SpeedState::new(0);
        m2.speed_limit = true;
        assert_eq!(m2.on_move(516, 0, &c), Verdict::Forward);

        let mut a = SpeedState::new(0);
        a.speed_limit = true;
        enqueue_delay(&mut a, 649);
        assert_eq!(a.on_attack(649, 0, &c), Verdict::Drop, "500+150=650");

        let mut sp = SpeedState::new(0);
        sp.speed_limit = true;
        enqueue_delay(&mut sp, 1_319);
        assert_eq!(sp.on_spell(1_319, 1, &c), Verdict::Drop, "1170+150=1320");
    }

    /// 非 `DelaySend` 的惩罚方式在这三条规则里 oracle **什么也不做** ⇒ 放行且不动计时。
    #[test]
    fn non_delay_punish_method_forwards_untouched() {
        let mut c = deployed();
        c.punish_method = PunishMethod::DropPack;
        let mut s = SpeedState::new(0);
        enqueue_delay(&mut s, 1);
        assert_eq!(s.on_move(1, 0, &c), Verdict::Forward);
        assert_eq!(s.move_tick, 0);
        assert_eq!(s.on_attack(1, 0, &c), Verdict::Forward);
        assert_eq!(s.attack_tick, 0);
        assert_eq!(s.on_spell(1, 1, &c), Verdict::Forward);
        assert_eq!(s.spell_tick, 0);
    }

    /// 三个 `[Switch]` 关掉时调用方仍可调规则函数，但必须无条件放行且不动计时。
    #[test]
    fn disabled_switches_forward_without_touching_state() {
        let mut c = deployed();
        c.move_interval_enabled = false;
        c.attack_interval_enabled = false;
        c.spell_interval_enabled = false;
        let mut s = SpeedState::new(0);
        enqueue_delay(&mut s, 0);
        assert_eq!(s.on_move(0, 0, &c), Verdict::Forward);
        assert_eq!(s.on_attack(0, 0, &c), Verdict::Forward);
        assert_eq!(s.on_spell(0, 1, &c), Verdict::Forward);
        assert_eq!(
            (s.move_tick, s.attack_tick, s.spell_tick),
            (0, 0, 0),
            "不计时不刷新"
        );
    }

    /// `PunishMethod` 的数值映射（`[Method] OverSpeedPunishMethod=3`）。
    #[test]
    fn punish_method_raw_values() {
        assert_eq!(PunishMethod::from_raw(0), PunishMethod::TurnPack);
        assert_eq!(PunishMethod::from_raw(1), PunishMethod::DropPack);
        assert_eq!(PunishMethod::from_raw(2), PunishMethod::NullPack);
        assert_eq!(PunishMethod::from_raw(3), PunishMethod::DelaySend);
        assert_eq!(PunishMethod::from_raw(99), PunishMethod::TurnPack);
    }

    /// 表常量与 oracle `TableDef.cs` 的锚点值（防抄错/错位）。
    #[test]
    fn magic_tables_match_oracle_anchor_values() {
        assert_eq!(MAGIC_NAMES.len(), 128);
        assert_eq!(MAGIC_DELAY_TIME_LIST_DEF[0], 60_000);
        assert_eq!(MAGIC_DELAY_TIME_LIST_DEF[1], 1_170); // 火球术 1110+60
        assert_eq!(MAGIC_DELAY_TIME_LIST_DEF[3], 1_110); // 基本剑术
        assert_eq!(MAGIC_DELAY_TIME_LIST_DEF[63], 200); // 噬魂沼泽
        assert_eq!(MAGIC_DELAY_TIME_LIST_DEF[112], 1_300); // 旋转风火轮
        assert!(!MAGIC_DELAY_ARRAY[7], "攻杀剑术不过网关施法间隔");
        assert!(MAGIC_DELAY_ARRAY[1]);
        assert!(MAGIC_ATTACK_ARRAY[1], "火球术算攻击魔法");
        assert!(!MAGIC_ATTACK_ARRAY[3], "基本剑术不算攻击魔法");
    }
}
