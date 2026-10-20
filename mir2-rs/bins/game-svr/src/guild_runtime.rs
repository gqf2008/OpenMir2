//! **行会 G3：持久化 + 10s/30s 门 + 断线 104/SS_208 + 清槽**（卡 `openmir2-m3-guild-persistence-g3`）。
//!
//! 唯一规格：`mir2-rs/tests/parity/m3/design-guild-g3-persistence-v9-20261020.md`（v9）。
//!
//! oracle 锚：
//! - 104 块：`src/M2Server/Player/PlayObject.Message.cs:367-389`（`Run()` 尾段，`!BoReconnection && BoSoftClose`
//!   时 `MyGuild = MemberOfGuild(ChrName)` 覆盖 ⇒ 104 + `SS_208`）；
//! - 清槽：`:2129-2150` 的 `Disappear()` ⇒ `MyGuild.DelHumanObj`（`GuildInfo.cs:534-550`，
//!   `Compare(...)==1`）；`DelHumanObj` 全仓唯一调用点；
//! - 10s/30s 门：`src/GameSrv/Word/Threads/SystemProcessor.cs:70-77` +
//!   `src/M2Server/Guild/GuildManager.cs:145-171`（`Run`）/ `GuildInfo.cs:525-533`（`CheckSaveGuildFile`）；
//! - `SS_208`：`src/GameSrv/Word/WorldServer.cs:885-895` 拼 `nCode/nServerIdx/sMsg`。
//!
//! ## C1：一个源
//!
//! `my_guild` 单变量：路径 A（`!bo_reconnection && bo_soft_close`）用 `member_of_guild` 覆盖并
//! **写回会话**（oracle `:382` 覆盖的就是 `PlayObject.MyGuild` 字段）；路径 B 用会话缓存。
//! 104 / `ss208` / check_save / 清槽**全部**用 `my_guild`，不再二次读会话。
//!
//! ## C2：收件人排除
//!
//! 104 的收件人由宿主闭包从 `online` + 实体解析（`guild_cmd::find_play_object` 已含
//! 在线/实体存在/`!ghost` 三重门），`on_disconnect` 再显式排除 `chr_name`（oracle 的
//! `MakeGhost` 让离场者收不到）。生产闭包**不得**捕获 `ground_sessions`（E0502）：宿主先
//! 快照 `ban` 表。

use mir2_data::guild::{GuildFileSections, GuildRecord, GuildStore, GuildStoreError, SaveStatus};
use mir2_protocol::messages;
use mir2_world::{GuildConfig, GuildManager, GuildRunEvent, GuildSaveReason, GuildState};

use crate::guild_cmd::GuildSessions;
use crate::guild_frames;

/// `SendGuildMsg` 的配置面（`GuildInfo.cs:431-456`）。
///
/// `show_pre_fix_msg` / `prefix` 来自 `[Setup] ShowPreFixMsg` + `[String] GuildMsgPreFix`；
/// 颜色**转发**自 [`GuildConfig`]（唯一来源，M-8：`[Setup] GuildMsgFColor/GuildMsgBColor`
/// 在 `config.rs` 写进 `GuildConfig`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GuildMsgCfg {
    /// `SystemShare.Config.ShowPreFixMsg`。
    pub show_pre_fix_msg: bool,
    /// `SystemShare.Config.GuildMsgPreFix`（缺省 `〖行会〗`）。
    pub prefix: String,
    /// `Param` 低字节。
    pub f_color: u8,
    /// `Param` 高字节。
    pub b_color: u8,
}

impl GuildMsgCfg {
    /// 由 `GuildConfig`（颜色唯一来源）+ `String.conf` 两键构造。
    #[must_use]
    pub(crate) fn from_config(
        guild_cfg: &GuildConfig,
        show_pre_fix_msg: bool,
        prefix: impl Into<String>,
    ) -> Self {
        Self {
            show_pre_fix_msg,
            prefix: prefix.into(),
            f_color: guild_cfg.guild_msg_f_color,
            b_color: guild_cfg.guild_msg_b_color,
        }
    }
}

/// `SS_208` 的三段（**不拼死串**，C3）：线上字节 = `"{ident}/{server_index}/{payload}"` 的 GBK。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServerGroupMsgPayload {
    /// `Messages.SS_208`（208）。
    pub ident: u16,
    /// `M2Share.ServerIndex`。
    pub server_index: i32,
    /// oracle 的 `sMsg` 实参：`"<会>//<名> has exited the game."`。
    pub payload: String,
}

impl ServerGroupMsgPayload {
    /// 可读日志。
    #[must_use]
    pub(crate) fn describe(&self) -> String {
        format!("SS_{}/{}/{}", self.ident, self.server_index, self.payload)
    }
}

/// 断线上下文（C1/C-1 之后的形状）。
#[derive(Clone, Debug)]
pub(crate) struct DisconnectCtx<'a> {
    /// 离场角色名（与被清槽/排收件人逐字比较）。
    pub chr_name: &'a str,
    /// `PlayObject.BoReconnection`。
    pub bo_reconnection: bool,
    /// `PlayObject.BoSoftClose`。
    pub bo_soft_close: bool,
    /// `world.clock_ms as i32`。
    pub now_tick: i32,
    /// `enter_cfg.server_index as i32`。
    pub server_index: i32,
}

/// `on_disconnect` 的可观测产出。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DisconnectOutcome {
    /// 104：`(收件人 actor_id, 已编码帧)`，rank/member 序。
    pub notices: Vec<(i32, Vec<u8>)>,
    /// `SS_208`（路径 A 且 `my_guild == Some`）。
    pub ss208: Option<ServerGroupMsgPayload>,
    /// `CheckSaveGuildFile` 命中时的落盘结果。
    pub saved: Option<Result<SaveStatus, GuildStoreError>>,
    /// 被清空槽的**旧成员名**。
    pub cleared: Option<String>,
}

/// 启动加载（`GuildManager.LoadGuildInfo`，`GuildManager.cs:78-110`）。
///
/// `store=None` ⇒ 空表（单测/无配置）。名单缺失 ⇒ 记 WRN、空表、不拒启（oracle
/// `if (File.Exists)` 直接留空）。
#[must_use]
pub(crate) fn startup(
    store: Option<&GuildStore>,
    sections: &GuildFileSections,
    rank_name_len: u8,
    now_tick: i32,
    server_index: i32,
) -> GuildManager {
    let mut manager = GuildManager::new();
    let Some(store) = store else {
        return manager;
    };
    match store.load_guild_info(sections, rank_name_len, now_tick, server_index) {
        Ok(load) => manager.load_guild_info(&load.guilds),
        Err(GuildStoreError::MissingListFile(path)) => crate::glog!(
            "WRN",
            "行会名单文件缺失：{} ⇒ 会表为空（oracle LoadGuildInfo 的 if (File.Exists) 直接留空）",
            path.display()
        ),
        Err(e) => crate::glog!("ERR", "行会加载失败：{e} ⇒ 会表为空（登记）"),
    }
    manager
}

/// 10s 门 + `GuildManager.Run` + 落盘（`SystemProcessor.cs:70-77` / `GuildManager.cs:145-171`）。
///
/// 门用 `now_tick.wrapping_sub(*run_tick) > 10_000`（严格 `>`，oracle `>`）。
/// WarEnded 走与断线同一 `104` 管道（`exclude=None`）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn step(
    guilds: &mut GuildManager,
    store: Option<&GuildStore>,
    sections: &GuildFileSections,
    msg_cfg: &GuildMsgCfg,
    server_index: i32,
    now_tick: i32,
    run_tick: &mut i32,
    recipients: &dyn Fn(&str) -> Option<(i32, bool)>,
) -> StepOutcome {
    let mut out = StepOutcome::default();
    if now_tick.wrapping_sub(*run_tick) <= 10_000 {
        return out;
    }
    *run_tick = now_tick;
    out.ran = true;
    for ev in guilds.run(now_tick) {
        out.events.push(ev.clone());
        match ev {
            GuildRunEvent::WarEnded { guild, message, .. } => {
                out.notices.extend(guild_broadcast_104(
                    guilds, &guild, &message, msg_cfg, recipients, None,
                ));
            }
            GuildRunEvent::Save { guild, reason } => {
                let Some(g) = guilds.find_guild(&guild) else {
                    continue;
                };
                let result = save_one(store, sections, g, now_tick, server_index);
                if let Err(e) = &result {
                    // `GuildInfo.cs:425-428`：写失败只记日志、不 panic、不重试。
                    crate::glog!("ERR", "保存行会信息失败!!! {e}");
                }
                out.saved.push((guild, reason, result));
            }
        }
    }
    out
}

/// `step` 的可读产出（不改 oracle 语义）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StepOutcome {
    /// 10s 门是否放行。
    pub ran: bool,
    /// 事件序（每会：`WarEnded*` → `Save{UpdateGuildFile}` → `Save{CheckSaveGuildFile}`）。
    pub events: Vec<GuildRunEvent>,
    /// WarEnded 的 104 帧。
    pub notices: Vec<(i32, Vec<u8>)>,
    /// `(会名, 原因, 落盘结果)`。
    pub saved: Vec<(String, GuildSaveReason, Result<SaveStatus, GuildStoreError>)>,
}

/// 断线生命周期（C1 单源 + C-1 路径门 + C2 排除）。
///
/// 固定序：**(0) 双门 → (1) 覆盖/取 `my_guild` → (2) 104 + SS_208（仅路径 A）→
/// (3) check_save（先落盘）→ (4) 清槽**。
#[allow(clippy::too_many_arguments)]
pub(crate) fn on_disconnect(
    guilds: &mut GuildManager,
    store: Option<&GuildStore>,
    sections: &GuildFileSections,
    msg_cfg: &GuildMsgCfg,
    sessions: &mut impl GuildSessions,
    ctx: &DisconnectCtx<'_>,
    recipients: &dyn Fn(&str) -> Option<(i32, bool)>,
) -> DisconnectOutcome {
    let mut out = DisconnectOutcome::default();
    // (0) 双门：无活会话 ⇒ 完全 no-op（drain_* 已移除会话后，同一角色 socket 关闭还会来
    // 一条 `Closed`）；`guild_done` ⇒ 只跑一次。两门都在覆盖之前。
    let Some(s) = sessions.guild(ctx.chr_name) else {
        return out;
    };
    if s.guild_done {
        return out;
    }
    let path_a = !ctx.bo_reconnection && ctx.bo_soft_close;
    let s = sessions
        .guild_mut(ctx.chr_name)
        .expect("上面已确认存在（同一张表）");
    s.guild_done = true;
    // (1) C1 一个源：只有路径 A 覆盖并写回会话字段（oracle `:382`）。
    let mut my_guild = s.guild_name.clone();
    if path_a {
        my_guild = guilds.member_of_guild(ctx.chr_name).map(str::to_string);
        s.guild_name = my_guild.clone();
    }
    let Some(g) = my_guild.as_deref() else {
        return out;
    };
    // (2) C-1：104 + SS_208 只在路径 A（oracle `:380` 的 `!BoReconnection && BoSoftClose`）。
    if path_a {
        out.notices = guild_broadcast_104(
            guilds,
            g,
            &format!("{} 已经退出游戏.", ctx.chr_name),
            msg_cfg,
            recipients,
            Some(ctx.chr_name),
        );
        out.ss208 = Some(ServerGroupMsgPayload {
            ident: messages::SS_208,
            server_index: ctx.server_index,
            payload: format!("{g}//{} has exited the game.", ctx.chr_name),
        });
    }
    // (3) 两条路径都做：先落盘（置 BoChanged=false，DwSaveTick 不动），再清槽。
    if let Some(guild) = guilds.find_guild_mut(g) {
        if guild.check_save_guild_file(ctx.now_tick) {
            out.saved = Some(save_one(
                store,
                sections,
                guild,
                ctx.now_tick,
                ctx.server_index,
            ));
            if let Some(Err(e)) = &out.saved {
                crate::glog!("ERR", "保存行会信息失败!!! {e}");
            }
        }
        out.cleared = guild.del_human_obj_member(ctx.chr_name);
    }
    out
}

/// `SendGuildMsg` 的 104 扇出（`GuildInfo.cs:431-456`）。
///
/// 逐 rank/逐 member：空名跳过（oracle `PlayObject == null`）→ 显式排除离场者（C2，
/// oracle 的 `!Ghost`）→ 宿主闭包解析（在线 + 实体存在 + `!ghost`）→ `BanGuildChat`
/// 反向门（`true` = 允许）→ 编码 104。前缀在 `ShowPreFixMsg` 为真时**无空格**拼接。
pub(crate) fn guild_broadcast_104(
    guilds: &GuildManager,
    guild_name: &str,
    text: &str,
    cfg: &GuildMsgCfg,
    recipients: &dyn Fn(&str) -> Option<(i32, bool)>,
    exclude: Option<&str>,
) -> Vec<(i32, Vec<u8>)> {
    let Some(guild) = guilds.find_guild(guild_name) else {
        return Vec::new();
    };
    let text = if cfg.show_pre_fix_msg {
        format!("{}{}", cfg.prefix, text)
    } else {
        text.to_string()
    };
    let mut out = Vec::new();
    for rank in &guild.ranks {
        for m in &rank.members {
            if m.is_empty() {
                continue;
            }
            if exclude == Some(m.as_str()) {
                continue;
            }
            let Some((actor_id, allow)) = recipients(m) else {
                continue;
            };
            if !allow {
                continue;
            }
            if let Some(body) =
                guild_frames::sm_guild_message_104(actor_id, cfg.f_color, cfg.b_color, &text)
            {
                out.push((actor_id, body));
            }
        }
    }
    out
}

/// `GuildInfo.SaveGuildInfoFile` 的落盘（`GuildStore` 层；`store=None` ⇒ 内存宿主 no-op）。
///
/// **不吞错误**：写失败由调用方按 oracle `GuildInfo.cs:425-428` 记日志。`MissingSaveTarget`
/// 因此可被 W3-0 变体 2 直接断言。
pub(crate) fn save_one(
    store: Option<&GuildStore>,
    sections: &GuildFileSections,
    guild: &GuildState,
    now_tick: i32,
    server_index: i32,
) -> Result<SaveStatus, GuildStoreError> {
    save_record(store, sections, &guild.to_record(), now_tick, server_index)
}

/// 落盘一条已物化记录（`del_guild` 的返回值没有活 `GuildState`，走本函数）。
pub(crate) fn save_record(
    store: Option<&GuildStore>,
    sections: &GuildFileSections,
    record: &GuildRecord,
    now_tick: i32,
    server_index: i32,
) -> Result<SaveStatus, GuildStoreError> {
    let Some(store) = store else {
        return Ok(SaveStatus::Written);
    };
    store.save_guild(record, sections, now_tick, server_index)
}

/// `GuildManager.SaveGuildList`（`GuildManager.cs:124-143`）：非 0 区 no-op。
pub(crate) fn save_list(
    store: Option<&GuildStore>,
    names: &[String],
    server_index: i32,
) -> Result<SaveStatus, GuildStoreError> {
    let Some(store) = store else {
        return Ok(SaveStatus::Written);
    };
    store.save_guild_list(names, server_index)
}

#[cfg(test)]
mod tests;
