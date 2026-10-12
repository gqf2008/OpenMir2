//! **取角色的控制流** —— C# `GameSrv/Services/FrnEngn.cs`（加载队列）+ `GameSrv/Word/Threads/CharacterDataProcessor.cs`
//! （读档循环）。本切片只做"控制流"，不碰 DB 线上协议（那是 S2b-2b）。
//!
//! 逐条对照：
//! - `AddToLoadRcdList`（`FrnEngn.cs:138-149`）：往 `m_LoadRcdList` 追加；
//! - `ProcessGameDate`（`:76-118`）的 **load 半边**：把 `m_LoadRcdList` 与 `m_LoadRcdTempList` **交换**（`:89-91`）
//!   ⇒ 消费者拿 temp、生产者继续塞 load，互不阻塞；
//! - `InSaveRcdList`（`:156-176`）：按 `ChrName` **OrdinalIgnoreCase** 线性找；
//! - `ProcessReadStorage`（`CharacterDataProcessor.cs:95-147`）+ `LoadCharacterData`（`:149-182`）：
//!   `SessionID == 0` 跳过（`:102-105`）；**保存中** ⇒ 重排队（`:112-121`，日志"角色数据保存中,稍后重新读取."）；
//!   否则 `QueryCharacterData(...)`（`:165`）：失败 ⇒ `SendOutConnectMsg` + 调用方 `NetChannel.CloseUser`
//!   + 日志"读取用户数据失败,踢出用户."（`:125-126`）；成功 ⇒ `AddUserOpenInfo`（`:171-178`，交给世界侧开人）；
//! - **注意 `SendOutConnectMsg` 在 oracle 里是"待实现"**：`TCP/TCPNetChannel.cs:388-402` 拼好了消息但**发送那句被注释掉**、
//!   只 `LogService.Info("待实现")` ⇒ 我们这一支也**不发帧**（只清槽 + 日志）。
//!
//! **登记不做（S2b-2b）**：真正的 DB 请求——`PlayerDataService.LoadRcd`（`:166-181`）→
//! `GameShare.DataServer.SendRequest`（`DataQueryServer.cs:62-90`：`ServerDataPacket` 头 + `EDCode(ServerRequestMessage)`
//! + `EDCode(LoadCharacterData)` + `Sign`），以及结果回收 `ProcessQueryQueue`（`:141-163`）。

/// 一次"取角色"请求（照 `ChannelMessageHandler.cs:323-337` 构造 `LoadDBInfo` 时填的字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadDbInfo {
    pub account: String,
    pub chr_name: String,
    pub ipaddr: String,
    /// 认证链里的 `nSessionId`（`SessionID`）。
    pub session_id: i32,
    /// `SoftVersionDate`（客户端版本）。
    pub soft_version_date: i32,
    pub pay_ment: i32,
    pub pay_mode: i32,
    /// `SocketId` = 客户端 socket 句柄（`CloseUser`/踢人用它）。
    pub socket_id: i32,
    /// `GSocketIdx` = 网关侧的 `SessionId`。
    pub gate_socket_idx: u16,
    /// `GateIdx` = 该网关连接的线号（`ClientSession` 的 `ServiceId`）。
    pub gate_idx: i32,
    /// `NewUserTick`（`HUtil32.GetTickCount`，本切片按调用方传入的毫秒数填）。
    pub new_user_tick: i64,
    pub play_time: i64,
}

/// 读档循环对单个用户的处置（`CharacterDataProcessor.cs:106-143` 的三个出口）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadAction {
    /// 保存中 ⇒ 重排队（日志照 `:121`）。
    RequeueSaving { account: String, chr_name: String },
    /// 取数失败 ⇒ `CloseUser` + 日志照 `:126`（oracle 的 `SendOutConnectMsg` 是"待实现"、不发帧）。
    KickNoData {
        gate_idx: i32,
        socket_id: i32,
        account: String,
        chr_name: String,
    },
    /// 取数成功 ⇒ 交给世界侧开人（`AddUserOpenInfo`，`:171-178`；进世界属 S2b-2b/3）。
    StartQuery {
        query_id: i32,
        account: String,
        chr_name: String,
    },
}

/// `FrnEngn` 的加载队列（双缓冲）+ 保存名单（`InSaveRcdList` 的判据）。
#[derive(Debug, Default)]
pub struct FrontEngine {
    /// `m_LoadRcdList`（生产者侧）
    load_list: Vec<LoadDbInfo>,
    /// `m_LoadRcdTempList`（消费者侧）
    load_temp: Vec<LoadDbInfo>,
    /// `m_SaveRcdList` 里本切片只用得到 `ChrName`（`AddToSaveRcdList` 的口径）
    save_list: Vec<String>,
}

impl FrontEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `AddToLoadRcdList`（`FrnEngn.cs:138-149`）。
    pub fn add_to_load_rcd_list(&mut self, info: LoadDbInfo) {
        self.load_list.push(info);
    }

    /// `FrnEngn.DeleteHuman`（`FrnEngn.cs:208-226`）：按 `(GateIdx, SocketId)` 把**未完成的**读档项
    /// 从 `m_LoadRcdList` 里摘掉（`CloseUser` 时调用）。这里按 `socket_id` 匹配就够——
    /// 一个 socket 只属于一条网关连接，且本骨架的 load/temp 双缓冲在 [`Self::drain_read_storage`]
    /// 里会把项搬去 `load_temp` ⇒ **两边都要清**，否则下一轮 `swap_load_lists` 又把它搬回来。
    ///
    /// 返回被摘掉的条数（=0 表示本来就没有待读项，不是错误）。
    pub fn delete_human(&mut self, socket_id: i32) -> usize {
        let before = self.load_list.len() + self.load_temp.len();
        self.load_list.retain(|i| i.socket_id != socket_id);
        self.load_temp.retain(|i| i.socket_id != socket_id);
        before - (self.load_list.len() + self.load_temp.len())
    }

    /// `InSaveRcdList`（`:156-176`）：**OrdinalIgnoreCase** 比较。
    #[must_use]
    pub fn in_save_rcd_list(&self, chr_name: &str) -> bool {
        self.save_list
            .iter()
            .any(|c| c.eq_ignore_ascii_case(chr_name))
    }

    /// 保存侧入队（`AddToSaveRcdList`；**存档回写属 S2b-3**，本切片先只有单测用）。
    #[allow(dead_code)]
    pub fn begin_save(&mut self, chr_name: &str) {
        self.save_list.push(chr_name.to_string());
    }

    /// `FrnEngn.IsFull()`（`:120-133`）：oracle 的实现是 `m_SaveRcdList.Count >= int.MaxValue`
    /// ⇒ **实际恒 `false`**（1:1 搬，不“修”成有意义的容量上限；周期存档的 `!IsFull()` 门用到它，
    /// `WorldServer.cs:698`）。
    #[must_use]
    pub fn is_full(&self) -> bool {
        // 本宿主 `save_list` 是 `Vec<String>`，长度不可能 >= `i32::MAX`。
        self.save_list.len() >= usize::try_from(i32::MAX).unwrap_or(usize::MAX)
    }

    /// 保存侧出队（`RemoveSaveList`/`ClearSaveList` 的简化：按名字删第一条）。
    #[allow(dead_code)]
    pub fn end_save(&mut self, chr_name: &str) {
        if let Some(i) = self
            .save_list
            .iter()
            .position(|c| c.eq_ignore_ascii_case(chr_name))
        {
            self.save_list.remove(i);
        }
    }

    /// `ProcessGameDate`（`:76-118`）的 load 半边：交换 load/temp（`:89-91`）。
    fn swap_load_lists(&mut self) {
        std::mem::swap(&mut self.load_temp, &mut self.load_list);
    }

    /// 一轮读档：交换 → 逐个判定 → 清 temp（`ProcessReadStorage` `:95-147`）。
    ///
    /// `query` 是"向 DB 取该角色数据"的接缝（oracle 的 `QueryCharacterData` `:165`）：
    /// 返回 `Some(query_id)` = 请求已发出（oracle 随后 `AddUserOpenInfo`）；`None` = 取数失败 ⇒ 踢。
    /// **本切片传的闭包恒返回 `None`**（还没有 DB 链路）⇒ 行为等价于"DBSvr 未连接"时 oracle 的做法。
    pub fn drain_read_storage(
        &mut self,
        mut query: impl FnMut(&LoadDbInfo) -> Option<i32>,
    ) -> Vec<LoadAction> {
        self.swap_load_lists();
        let temp = std::mem::take(&mut self.load_temp);
        let mut actions = Vec::with_capacity(temp.len());
        for info in &temp {
            if info.session_id == 0 {
                continue; // `:102-105`
            }
            if self.in_save_rcd_list(&info.chr_name) {
                // `:154-158` ⇒ 调用方重排队（`:112-121`）
                self.load_list.push(info.clone());
                actions.push(LoadAction::RequeueSaving {
                    account: info.account.clone(),
                    chr_name: info.chr_name.clone(),
                });
                continue;
            }
            match query(info) {
                Some(query_id) => actions.push(LoadAction::StartQuery {
                    query_id,
                    account: info.account.clone(),
                    chr_name: info.chr_name.clone(),
                }),
                None => actions.push(LoadAction::KickNoData {
                    gate_idx: info.gate_idx,
                    socket_id: info.socket_id,
                    account: info.account.clone(),
                    chr_name: info.chr_name.clone(),
                }),
            }
        }
        actions // temp 已被 take ⇒ 等价于 `ClearLoadRcdTempList`（`:145`）
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `CloseUser` 的 `FrontEngine.DeleteHuman`：`delete_human` 必须把未读项从 **load 与 temp 两边**
    /// 都摘掉（只清 load 侧、下一轮 `swap_load_lists` 会把 temp 里的搬回来 ⇒ 连接断了还会去 DBSvr 取档）。
    #[test]
    fn delete_human_removes_pending_load_on_both_buffers() {
        let mut e = FrontEngine::new();
        e.load_list.push(info("a", "hero_a", 5));
        e.load_temp.push(info("b", "hero_b", 6));
        assert_eq!(e.delete_human(4242), 2, "load 与 temp 两边各一条都要摘掉");
        assert!(e.load_list.is_empty());
        assert!(e.load_temp.is_empty());
        assert_eq!(e.delete_human(4242), 0, "幂等：再删没有可删的");
        // 只按 socket 匹配：别的 socket 不动
        e.load_list.push(LoadDbInfo {
            socket_id: 7,
            ..info("c", "hero_c", 7)
        });
        assert_eq!(e.delete_human(4242), 0);
        assert_eq!(e.load_list.len(), 1, "socket_id=7 的项不得被误删");
    }

    fn info(account: &str, chr: &str, session_id: i32) -> LoadDbInfo {
        LoadDbInfo {
            account: account.to_string(),
            chr_name: chr.to_string(),
            ipaddr: "127.0.0.1".to_string(),
            session_id,
            soft_version_date: 120020522,
            pay_ment: 1,
            pay_mode: 1,
            socket_id: 4242,
            gate_socket_idx: 77,
            gate_idx: 0,
            new_user_tick: 0,
            play_time: 3600,
        }
    }

    #[test]
    fn double_buffer_delivers_each_request_once() {
        let mut e = FrontEngine::new();
        e.add_to_load_rcd_list(info("a", "hero_a", 5));
        e.add_to_load_rcd_list(info("b", "hero_b", 6));
        let acts = e.drain_read_storage(|_| Some(11));
        assert_eq!(acts.len(), 2, "两个请求各出一条动作");
        assert!(matches!(
            acts[0],
            LoadAction::StartQuery { query_id: 11, .. }
        ));
        assert!(
            acts[1].clone()
                == LoadAction::StartQuery {
                    query_id: 11,
                    account: "b".into(),
                    chr_name: "hero_b".into()
                }
        );
        // 再 drain 一次：temp 已清 ⇒ 无动作（不会重复处理）
        assert!(e.drain_read_storage(|_| Some(11)).is_empty());
    }

    #[test]
    fn saving_character_is_requeued_then_processed_next_round() {
        let mut e = FrontEngine::new();
        e.begin_save("Hero_A"); // 大小写不同也命中（OrdinalIgnoreCase）
        e.add_to_load_rcd_list(info("a", "hero_a", 5));
        let acts = e.drain_read_storage(|_| Some(1));
        assert_eq!(
            acts,
            vec![LoadAction::RequeueSaving {
                account: "a".into(),
                chr_name: "hero_a".into()
            }]
        );
        // 保存结束后下一轮才真取数
        e.end_save("hero_a");
        let acts2 = e.drain_read_storage(|_| Some(9));
        assert_eq!(
            acts2,
            vec![LoadAction::StartQuery {
                query_id: 9,
                account: "a".into(),
                chr_name: "hero_a".into()
            }]
        );
    }

    #[test]
    fn query_failure_kicks_and_session_zero_is_skipped() {
        let mut e = FrontEngine::new();
        e.add_to_load_rcd_list(info("a", "hero_a", 0)); // SessionID == 0 ⇒ 跳过（`:102-105`）
        e.add_to_load_rcd_list(info("b", "hero_b", 6));
        let acts = e.drain_read_storage(|_| None); // DB 链路不在 ⇒ 取数失败
        assert_eq!(
            acts,
            vec![LoadAction::KickNoData {
                gate_idx: 0,
                socket_id: 4242,
                account: "b".into(),
                chr_name: "hero_b".into()
            }]
        );
    }

    #[test]
    fn in_save_list_case_insensitive_and_removable() {
        let mut e = FrontEngine::new();
        assert!(!e.in_save_rcd_list("hero"));
        e.begin_save("HERO");
        assert!(e.in_save_rcd_list("Hero"), "OrdinalIgnoreCase");
        e.end_save("hero");
        assert!(!e.in_save_rcd_list("HERO"));
    }
}
