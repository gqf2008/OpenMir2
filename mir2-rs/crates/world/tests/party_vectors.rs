//! 组队（party）状态机的 oracle 边界向量 —— M3⑦-实现 2（卡 `openmir2-m3-party-group-state`）。
//!
//! 断言逐条对外 oracle 的**源码调用序**与 **`Run()` 出队序**（见 `crates/world/src/party.rs` 模块头）：
//! 同一目标的线上帧 = 立即帧（源码序）→ 排队帧（`(byte)wIdent` 升序）。
//!
//! oracle 锚点：`PlayObject.Operate.cs:787-909`、`PlayObject.cs:5126-5197`、
//! `PlayObject.Base.cs:1339-1369/2418-2422`、`PlayObject.Message.cs:704-719/914-935`。

use mir2_world::{Entity, PartyConfig, PartyEvent, World};

fn cfg() -> PartyConfig {
    PartyConfig::default()
}

fn player(w: &mut World, id: i32, name: &str) -> i32 {
    w.add_entity(Entity::new(id, name, 10, 10))
}

/// 造 3 名玩家（L/T/U），并让 T/U 打开 `AllowGroup`（模拟先发 `CM_GROUPMODE` 且 `param2 != 0`）。
fn three_players() -> (World, i32, i32, i32) {
    let mut w = World::new(32, 32);
    let l = player(&mut w, 1, "L");
    let t = player(&mut w, 2, "T");
    let u = player(&mut w, 3, "U");
    for id in [t, u] {
        w.entities.get_mut(id).unwrap().allow_group = true;
    }
    (w, l, t, u)
}

fn sys(target: i32, recog: i32, text: &str) -> PartyEvent {
    PartyEvent::SysMessage {
        target,
        recog,
        f_color: 0xC4,
        b_color: 0xFF,
        text: text.to_string(),
    }
}

#[test]
fn create_group_matches_oracle_call_order_and_dequeue_order() {
    let (mut w, l, t, _u) = three_players();
    let evs = w.party_create_group(l, "T", &cfg());

    let expected = vec![
        // 立即帧（Operate.cs:840/841-842）
        PartyEvent::CreateGroupOk { target: l },
        PartyEvent::GroupMembers {
            target: l,
            members_text: "L/T/".into(),
        },
        PartyEvent::GroupMembers {
            target: t,
            members_text: "L/T/".into(),
        },
        // 排队帧（RM_GROUPMESSAGE，优先级 253；入队序 = JoinGroup(L) 再 JoinGroup(T)）
        sys(l, l, "〖组队〗L 已加入小组."),
        sys(t, l, "〖组队〗L 已加入小组."),
        sys(l, t, "〖组队〗T 已加入小组."),
        sys(t, t, "〖组队〗T 已加入小组."),
    ];
    assert_eq!(evs, expected);

    // 状态：团长 L 的成员表含自己 + T，两方 GroupOwner 都是 L，L 的 AllowGroup 置 true
    let le = w.entities.get(l).unwrap();
    assert_eq!(le.group_members, vec![l, t]);
    assert!(le.allow_group);
    assert_eq!(w.entities.get(t).unwrap().group_owner, l);
    assert_eq!(le.group_owner, l);
}

#[test]
fn create_group_fail_codes_are_oracle_exact() {
    // -2：目标不存在
    let (mut w, l, _t, _u) = three_players();
    assert_eq!(
        w.party_create_group(l, "NOPE", &cfg()),
        vec![PartyEvent::CreateGroupFail {
            target: l,
            recog: -2
        }]
    );
    // -2：目标是自己
    assert_eq!(
        w.party_create_group(l, "L", &cfg()),
        vec![PartyEvent::CreateGroupFail {
            target: l,
            recog: -2
        }]
    );
    // -2：目标 Death
    let (mut w, l, t, _u) = three_players();
    w.entities.get_mut(t).unwrap().death = true;
    assert_eq!(
        w.party_create_group(l, "T", &cfg()),
        vec![PartyEvent::CreateGroupFail {
            target: l,
            recog: -2
        }]
    );
    // -4：目标未开 AllowGroup（构造时 T 未开）
    let mut w = World::new(32, 32);
    let l = player(&mut w, 1, "L");
    let _t = player(&mut w, 2, "T");
    assert_eq!(
        w.party_create_group(l, "T", &cfg()),
        vec![PartyEvent::CreateGroupFail {
            target: l,
            recog: -4
        }]
    );
    // -3：目标已在别人组里
    let (mut w, l, t, _u) = three_players();
    w.entities.get_mut(t).unwrap().group_owner = 99;
    assert_eq!(
        w.party_create_group(l, "T", &cfg()),
        vec![PartyEvent::CreateGroupFail {
            target: l,
            recog: -3
        }]
    );
    // -1：自己已在组里
    let (mut w, l, _t, _u) = three_players();
    assert!(matches!(
        w.party_create_group(l, "T", &cfg()).first(),
        Some(PartyEvent::CreateGroupOk { .. })
    ));
    assert_eq!(
        w.party_create_group(l, "T", &cfg()),
        vec![PartyEvent::CreateGroupFail {
            target: l,
            recog: -1
        }]
    );
}

#[test]
fn add_member_uses_strict_greater_than_max() {
    let (mut w, l, t, u) = three_players();
    let mut c = cfg();
    c.group_members_max = 2;
    w.party_create_group(l, "T", &c);
    // Count == 2，`2 > 2` 假 ⇒ 允许加入（oracle `Operate.cs:855` 的严格大于）
    let evs = w.party_add_group_member(l, "U", &c);
    assert!(matches!(evs.first(), Some(PartyEvent::AddMemberOk { target }) if *target == l));
    assert_eq!(w.entities.get(l).unwrap().group_members, vec![l, t, u]);
    // Count == 3 ⇒ `3 > 2` 真 ⇒ -5
    assert_eq!(
        w.party_add_group_member(l, "U", &c),
        vec![PartyEvent::AddMemberFail {
            target: l,
            recog: -5
        }]
    );
}

#[test]
fn del_member_regular_keeps_leader_with_cancel_and_delmem_ok() {
    let (mut w, l, t, _u) = three_players();
    w.party_create_group(l, "T", &cfg());
    let evs = w.party_del_group_member(l, "T", &cfg());

    // 立即：CancelGroup()==false ⇒ SM_GROUPCANCEL(L)，随后 SM_GROUPDELMEM_OK(L, T)
    // 排队（优先级升序）：RM_GROUPCANCEL(T)=156 排在 RM_GROUPMESSAGE(253) 之前
    let expected = vec![
        PartyEvent::GroupCancel { target: l },
        PartyEvent::DelMemberOk {
            target: l,
            name: "T".into(),
        },
        PartyEvent::GroupCancel { target: t },
        sys(l, t, "〖组队〗T 已经退出了本组."),
        sys(t, t, "〖组队〗T 已经退出了本组."),
        sys(l, l, "〖组队〗你的小组被解散了."),
    ];
    assert_eq!(evs, expected);

    // 状态：L 的组被解散（成员空、GroupOwner=0），T 的 GroupOwner=0
    let le = w.entities.get(l).unwrap();
    assert!(le.group_members.is_empty());
    assert_eq!(le.group_owner, 0);
    assert_eq!(w.entities.get(t).unwrap().group_owner, 0);
}

#[test]
fn del_member_leader_dissolves_whole_group() {
    let (mut w, l, t, u) = three_players();
    w.party_create_group(l, "T", &cfg());
    w.party_add_group_member(l, "U", &cfg());
    assert_eq!(w.entities.get(l).unwrap().group_members, vec![l, t, u]);

    // 团长自己退组（oracle `OnDie`/`Disappear` 走 `owner.DelMember(this)`，this==owner）
    let evs = w.party_del_member(l, l, &cfg());
    // 每个成员各一条排队 RM_GROUPCANCEL(156)；团长自己的 LeaveGroup 先清了自己的 GroupOwner，
    // 故 CancelGroup 的 SendGroupText 无收件人（owner 0）⇒ 无「解散」文案；
    // 最后立即 SM_GROUPCANCEL(L)（CancelGroup 返回 false）。
    let mut got: Vec<&PartyEvent> = evs.iter().collect();
    got.sort_by_key(|e| format!("{e:?}"));
    assert!(evs.contains(&PartyEvent::GroupCancel { target: l }));
    assert!(evs.contains(&PartyEvent::GroupCancel { target: t }));
    assert!(evs.contains(&PartyEvent::GroupCancel { target: u }));
    assert!(!evs
        .iter()
        .any(|e| matches!(e, PartyEvent::SysMessage { text, .. } if text.contains("解散"))));
    for id in [l, t, u] {
        assert_eq!(w.entities.get(id).unwrap().group_owner, 0);
    }
    assert!(w.entities.get(l).unwrap().group_members.is_empty());
    let _ = got;
}

#[test]
fn group_close_non_leader_calls_del_member_and_clears_allow_group() {
    let (mut w, l, t, _u) = three_players();
    w.party_create_group(l, "T", &cfg());
    let evs = w.party_group_close(t, &cfg());
    // 与 `del_member_em(L, T)` 同序（但没有 CM_GROUPDELMEM 的回包）
    assert_eq!(
        evs,
        vec![
            PartyEvent::GroupCancel { target: l },
            PartyEvent::GroupCancel { target: t },
            sys(l, t, "〖组队〗T 已经退出了本组."),
            sys(t, t, "〖组队〗T 已经退出了本组."),
            sys(l, l, "〖组队〗你的小组被解散了."),
        ]
    );
    assert!(!w.entities.get(t).unwrap().allow_group);
}

#[test]
fn group_close_leader_emits_red_sysmsg_only() {
    let (mut w, l, _t, _u) = three_players();
    w.party_create_group(l, "T", &cfg());
    let evs = w.party_group_close(l, &cfg());
    assert_eq!(
        evs,
        vec![PartyEvent::SysMessage {
            target: l,
            recog: l,
            f_color: 0xFF,
            b_color: 0x38,
            text: "如果你想退出，使用编组功能（删除按钮）".into(),
        }]
    );
    // 团长仍保留组
    assert_eq!(w.entities.get(l).unwrap().group_owner, l);
}

#[test]
fn group_mode_sets_allow_group_and_reports_param() {
    let mut w = World::new(32, 32);
    let p = player(&mut w, 1, "P");
    // param2 != 0 ⇒ AllowGroup=true ⇒ SM_GROUPMODECHANGED param=1
    assert_eq!(
        w.party_group_mode(p, 1, &cfg()),
        vec![PartyEvent::GroupModeChanged {
            target: p,
            param: 1
        }]
    );
    assert!(w.entities.get(p).unwrap().allow_group);
    // param2 == 0 且 GroupOwner==0 ⇒ ClientGroupClose 只清 AllowGroup ⇒ param=0
    assert_eq!(
        w.party_group_mode(p, 0, &cfg()),
        vec![PartyEvent::GroupModeChanged {
            target: p,
            param: 0
        }]
    );
    assert!(!w.entities.get(p).unwrap().allow_group);
}

#[test]
fn run_tick_clears_owner_when_leader_dead_and_removes_dead_member() {
    let (mut w, l, t, u) = three_players();
    w.party_create_group(l, "T", &cfg());
    w.party_add_group_member(l, "U", &cfg());

    // 团长死亡 ⇒ 非团长成员的 GroupOwner 清 0（oracle Message.cs:704-713）
    w.entities.get_mut(l).unwrap().death = true;
    w.party_run_tick();
    for id in [l, t, u] {
        assert_eq!(w.entities.get(id).unwrap().group_owner, 0);
    }

    // 复活团长 + 重建组；成员 T 死亡 ⇒ 从团长成员表移除（Message.cs:715-726），GroupOwner 不变
    let (mut w, l, t, u) = three_players();
    w.party_create_group(l, "T", &cfg());
    w.party_add_group_member(l, "U", &cfg());
    w.entities.get_mut(t).unwrap().death = true;
    w.party_run_tick();
    assert_eq!(w.entities.get(l).unwrap().group_members, vec![l, u]);
    // 成员 T 的 GroupOwner 仍指向 L（团长没死，本侧不清）
    assert_eq!(w.entities.get(t).unwrap().group_owner, l);
}

#[test]
fn is_group_member_matches_leader_membership() {
    let (mut w, l, t, u) = three_players();
    assert!(!w.party_is_group_member(l, t));
    w.party_create_group(l, "T", &cfg());
    assert!(w.party_is_group_member(l, t));
    assert!(!w.party_is_group_member(l, u));
    assert!(w.party_is_group_member(t, l));
    assert!(w.party_is_group_member(t, t));
}

#[test]
fn add_member_fail_codes_are_oracle_exact() {
    let (mut w, l, t, u) = three_players();
    // -1：非团长
    assert_eq!(
        w.party_add_group_member(t, "U", &cfg()),
        vec![PartyEvent::AddMemberFail {
            target: t,
            recog: -1
        }]
    );
    w.party_create_group(l, "T", &cfg());
    // -2：目标不存在
    assert_eq!(
        w.party_add_group_member(l, "NOPE", &cfg()),
        vec![PartyEvent::AddMemberFail {
            target: l,
            recog: -2
        }]
    );
    // -2：自己
    assert_eq!(
        w.party_add_group_member(l, "L", &cfg()),
        vec![PartyEvent::AddMemberFail {
            target: l,
            recog: -2
        }]
    );
    // -3：目标已有组
    w.entities.get_mut(u).unwrap().group_owner = 77;
    assert_eq!(
        w.party_add_group_member(l, "U", &cfg()),
        vec![PartyEvent::AddMemberFail {
            target: l,
            recog: -3
        }]
    );
}

#[test]
fn del_group_member_fail_codes_are_oracle_exact() {
    let (mut w, l, t, _u) = three_players();
    // -1：非团长
    assert_eq!(
        w.party_del_group_member(t, "L", &cfg()),
        vec![PartyEvent::DelMemberFail {
            target: t,
            recog: -1
        }]
    );
    w.party_create_group(l, "T", &cfg());
    // -2：目标不存在
    assert_eq!(
        w.party_del_group_member(l, "NOPE", &cfg()),
        vec![PartyEvent::DelMemberFail {
            target: l,
            recog: -2
        }]
    );
    // -3：不是本组成员
    assert_eq!(
        w.party_del_group_member(l, "U", &cfg()),
        vec![PartyEvent::DelMemberFail {
            target: l,
            recog: -3
        }]
    );
}

// ---------------------------------------------------------------------------
// F-1（reviewer-5 `fc68afca`）：oracle `Die()` → `groupOwnerPlay.DelMember(this)`
// ---------------------------------------------------------------------------

/// **F-1**：成员死亡 ⇒ oracle `Die()`（`PlayObject.cs:1975-1977`）在死亡写入点调
/// `groupOwnerPlay.DelMember(this)`。本用例走**真实死亡写入点**（`hp == 0` + `World::tick()`
/// = `settle_deaths` / oracle `Die()`），不手工调 `DelMember` —— 这样接线若被删，用例必红。
///
/// 3 人组 ⇒ `CancelGroup()` 返回 true（`Count > 1`）⇒ 组**不散**、只摘死亡成员，且
/// `SendGroupMembers` 下发给其余成员；死亡者仍在线 ⇒ 它自己的退出说词 / `SM_GROUPCANCEL` 照发。
#[test]
fn die_member_triggers_oracle_del_member_and_group_survives() {
    let (mut w, l, t, u) = three_players();
    w.party_create_group(l, "T", &cfg());
    w.party_add_group_member(l, "U", &cfg());
    assert_eq!(w.entities.get(l).unwrap().group_members, vec![l, t, u]);

    // 走真实死亡写入点：`hp == 0`（oracle `Struck` → `Die()`；Rust `settle_deaths`）。
    w.entities.get_mut(t).unwrap().hp = 0;
    let rec = w.tick();

    assert!(w.entities.get(t).unwrap().death, "Die() 置 Death = true");
    assert_eq!(
        w.entities.get(t).unwrap().group_owner,
        0,
        "Die→DelMember→LeaveGroup 必须清死亡者 GroupOwner（reviewer 探针的根因）"
    );
    assert_eq!(
        w.entities.get(l).unwrap().group_members,
        vec![l, u],
        "团长成员表摘除死者，其余成员保留"
    );
    // `Count > 1` ⇒ `CancelGroup()` 返回 true ⇒ 组不散。
    assert_eq!(w.entities.get(l).unwrap().group_owner, l, "3 人组不散");
    assert_eq!(w.entities.get(u).unwrap().group_owner, l, "其余成员仍在组");

    // 事件面（立即帧 → 排队帧按 `(byte)wIdent` 升序）：
    // `SendGroupMembers`（立即，成员表已不含死者）→ `RM_GROUPCANCEL`(156) → `RM_GROUPMESSAGE`(253)。
    let expected = vec![
        PartyEvent::GroupMembers {
            target: l,
            members_text: "L/U/".into(),
        },
        PartyEvent::GroupMembers {
            target: u,
            members_text: "L/U/".into(),
        },
        PartyEvent::GroupCancel { target: t },
        // `SendGroupText(sender=T)`：`recog` = 发送者 T，收件人 = 当时组长成员表 [L,T,U]。
        sys(l, t, "〖组队〗T 已经退出了本组."),
        sys(t, t, "〖组队〗T 已经退出了本组."),
        sys(u, t, "〖组队〗T 已经退出了本组."),
    ];
    assert_eq!(rec.party_events, expected);

    // 复活后重新入组：`group_owner == 0` 是入组前置（`-3`），证明无残留死组员。
    w.entities.get_mut(t).unwrap().death = false;
    w.entities.get_mut(t).unwrap().allow_group = true;
    assert_eq!(
        w.party_add_group_member(l, "T", &cfg()),
        vec![
            PartyEvent::AddMemberOk { target: l },
            PartyEvent::GroupMembers {
                target: l,
                members_text: "L/U/T/".into(),
            },
            PartyEvent::GroupMembers {
                target: u,
                members_text: "L/U/T/".into(),
            },
            PartyEvent::GroupMembers {
                target: t,
                members_text: "L/U/T/".into(),
            },
            sys(l, t, "〖组队〗T 已加入小组."),
            sys(u, t, "〖组队〗T 已加入小组."),
            sys(t, t, "〖组队〗T 已加入小组."),
        ]
    );
}

/// **F-1**：团长死亡 ⇒ `Die()` 里 `groupOwnerPlay == 自己` ⇒ `DelMember(this)` 的团长支
/// （`PlayObject.cs:5143-5151`）：从尾到头逐成员 `LeaveGroup()` 后整组散。
#[test]
fn die_leader_disbands_group() {
    let (mut w, l, t, u) = three_players();
    w.party_create_group(l, "T", &cfg());
    w.party_add_group_member(l, "U", &cfg());

    w.entities.get_mut(l).unwrap().hp = 0;
    let rec = w.tick();

    for id in [l, t, u] {
        assert!(
            w.entities.get(id).unwrap().group_owner == 0,
            "团长死亡 ⇒ 全组 GroupOwner 清 0（id={id}）"
        );
    }
    assert!(w.entities.get(l).unwrap().group_members.is_empty());
    let cancels: Vec<i32> = rec
        .party_events
        .iter()
        .filter_map(|e| match e {
            PartyEvent::GroupCancel { target } => Some(*target),
            _ => None,
        })
        .collect();
    for id in [l, t, u] {
        assert!(
            cancels.contains(&id),
            "每个成员都收 SM_GROUPCANCEL（id={id}，实际 {cancels:?}）"
        );
    }
}

/// **M-1**（reviewer-5 `fc68afca` Minor）：oracle `WorldEngine.GetPlayObject(name)` 只扫
/// `PlayObjectList`（仅玩家）；本仓实体表含怪物/NPC 且都带 `name` ⇒ 必须按 `Race == Play 过滤。
#[test]
fn play_object_by_name_ignores_non_player_entities() {
    // 情形 1：玩家 T(2) 与怪物 T(50) 同名 ⇒ 命中玩家。
    let (mut w, _l, t, _u) = three_players();
    let mut mon_same_name = Entity::new(50, "T", 10, 10);
    mon_same_name.race = 1; // 非 Play
    w.add_entity(mon_same_name);
    assert_eq!(w.party_play_object_by_name("T"), Some(t));

    // 情形 2：同名者只有怪物 ⇒ oracle 判 null（`-2`），Rust 也必须 `-2`（不再误命中怪物）。
    let mut w = World::new(32, 32);
    let l = player(&mut w, 1, "L");
    let mut mon = Entity::new(50, "BOSS", 10, 10);
    mon.race = 1;
    w.add_entity(mon);
    assert_eq!(w.party_play_object_by_name("BOSS"), None);
    assert_eq!(
        w.party_create_group(l, "BOSS", &cfg()),
        vec![PartyEvent::CreateGroupFail {
            target: l,
            recog: -2
        }]
    );
}

/// §1.O `recall-ext` 读侧宿主快照：`(GroupOwner, GroupMembers[i].Job)`。
///
/// oracle 锚：`ConditionProcessingSys.cs:2670/262/4466` 读 `playerActor.GroupOwner` /
/// `playerActor.GroupMembers[i].Job`；`GroupMembers` 只挂在团长实体上
/// （`PlayObject.Operate.cs:834-835/875`）。非团长 ⇒ 空表。
#[test]
fn script_group_snapshot_reads_owner_and_member_jobs() {
    let (mut w, l, t, u) = three_players();
    // 三种职业各一名：L=战士(0)、T=法师(1)、U=道士(2)。
    w.entities.get_mut(l).unwrap().job = 0;
    w.entities.get_mut(t).unwrap().job = 1;
    w.entities.get_mut(u).unwrap().job = 2;

    // 无组：owner=0、空表。
    assert_eq!(w.party_script_group_snapshot(l), (0, vec![]));

    // 建组后团长 L 持有成员表（含自己）⇒ job 序 = 实体槽位序。
    let _ = w.party_create_group(l, "T", &cfg());
    assert_eq!(w.party_script_group_snapshot(l), (l, vec![0, 1]));
    // 组员 T 的 GroupOwner 指向团长，但自己的成员表为空。
    assert_eq!(w.party_script_group_snapshot(t), (l, vec![]));
    // 未入组的 U 无组。
    assert_eq!(w.party_script_group_snapshot(u), (0, vec![]));
}
