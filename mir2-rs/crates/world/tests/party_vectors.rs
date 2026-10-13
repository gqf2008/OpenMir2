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
