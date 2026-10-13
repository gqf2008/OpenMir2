//! M3⑦-实现 4（卡 `openmir2-m3-party-attribution`）的 oracle 边界向量：
//! 归属/分配的组队分支 —— `HAM_GROUP` 的 `IsGroupMember` 剔除 + `GainExp` 组队分配。
//!
//! oracle 锚点：
//! - `PlayObject.Attack.cs:667-677`（`HAM_GROUP` 支的 `IsGroupMember` 剔除）；
//! - `PlayObject.cs:5158-5175`（`IsGroupMember`）；
//! - `PlayObject.Base.cs:764-818`（`GainExp`，含 `|CurrX-x|` **两次都比 `CurrX`、漏 `CurrY`** 的
//!   oracle 既有缺陷，照搬）；
//! - `src/OpenMir2/Grobal2.cs:35`（`GroupMax = 11`）、`PlayObject.Base.cs:766`（`bonus[12]`）。

use mir2_world::{
    is_attack_target_player_grouped, Entity, PartyConfig, World, ACTOR_RACE_MONSTER,
    ACTOR_RACE_PLAY, ATTACK_MODE_HAM_GROUP,
};

/// 3 名玩家 L/A/B 结成一组（L 团长）；`need_exps` 抬高避免升级，只看单次经验分配。
fn setup_group() -> (World, i32, i32, i32) {
    let mut w = World::new(32, 32);
    for v in w.need_exps.iter_mut() {
        *v = 1_000_000;
    }
    let l = w.add_entity(Entity::new(1, "L", 10, 10));
    let a = w.add_entity(Entity::new(2, "A", 11, 10));
    let b = w.add_entity(Entity::new(3, "B", 12, 10));
    for id in [a, b] {
        w.entities.get_mut(id).unwrap().allow_group = true;
    }
    let cfg = PartyConfig::default();
    let _ = w.party_create_group(l, "A", &cfg);
    let _ = w.party_add_group_member(l, "B", &cfg);
    (w, l, a, b)
}

/// 造一只 `hp=0`、`ExpHitter = hitter` 的怪，tick 一次；返回 `deaths[0]` 的经验值
/// （= `GainExp` 里**获得者本人**本次获得的经验）。
fn kill_monster(w: &mut World, hitter: i32, mon_exp: i32) -> i32 {
    let mon = {
        let mut e = Entity::new(0, "chicken", 10, 9);
        e.race = ACTOR_RACE_MONSTER;
        e.mon_exp = mon_exp;
        e.exp_hitter = Some(hitter);
        w.add_entity(e)
    };
    w.entities.get_mut(mon).unwrap().hp = 0;
    let rec = w.tick();
    assert_eq!(rec.deaths.len(), 1, "应有 1 次死亡结算: {rec:?}");
    rec.deaths[0].4
}

fn awards(w: &World, rec: &mir2_world::TickRecord) -> Vec<i32> {
    let _ = w;
    rec.exp_awards.iter().map(|(id, ..)| *id).collect()
}

/// `HighLevelKillMonFixExp = true`：`dwExp = Round(600 * bonus[3]) = 840`，`n = 3`
/// ⇒ 每人 `Round(840 / 3) = 280`（`Base.cs:793-796`）。
#[test]
fn group_exp_fix_splits_evenly_with_bonus() {
    let (mut w, l, a, b) = setup_group();
    let gained = kill_monster(&mut w, l, 600);
    assert_eq!(gained, 280, "deaths 5 元组 = 获得者本人的经验");
    assert_eq!(w.entities.get(l).unwrap().exp, 280);
    assert_eq!(w.entities.get(a).unwrap().exp, 280);
    assert_eq!(w.entities.get(b).unwrap().exp, 280);
}

/// `HighLevelKillMonFixExp = false`：按等级比 `Round(dwExp / sumlv * level)`（`Base.cs:803`）。
/// L=4 / A=3 / B=2 ⇒ sumlv=9，dwExp=840 ⇒ 373 / 280 / 187。
#[test]
fn group_exp_by_level_ratio() {
    let (mut w, l, a, b) = setup_group();
    w.combat_config.high_level_kill_mon_fix_exp = false;
    for (id, lvl) in [(l, 4u8), (a, 3), (b, 2)] {
        w.entities.get_mut(id).unwrap().level = lvl;
    }
    let gained = kill_monster(&mut w, l, 600);
    assert_eq!(gained, 373, "L: Round(840/9*4)");
    assert_eq!(w.entities.get(l).unwrap().exp, 373);
    assert_eq!(w.entities.get(a).unwrap().exp, 280, "A: Round(840/9*3)");
    assert_eq!(w.entities.get(b).unwrap().exp, 187, "B: Round(840/9*2)");
}

/// **oracle 缺陷向量（§1 硬约束③）**：`GainExp` 的两次距离判定都是
/// `Math.Abs(CurrX - playObject.CurrX) <= 12`，**第二次没有比 `CurrY`**
/// （`PlayObject.Base.cs:780/788`）⇒ B 与 L 的 X 相同、Y 相差 90 **仍被计入** `n`。
///
/// 改坏必红：把第二次判定改成 `|CurrY - y| <= 12` ⇒ B 被排除、`n = 2`，
/// B 的经验变 0、L 的经验变 `Round(Round(600*1.3)/2) = 390` ⇒ 本用例两条断言同时红。
#[test]
fn group_exp_replicates_oracle_curry_missing_bug() {
    let (mut w, l, a, b) = setup_group();
    w.entities.get_mut(b).unwrap().y = 100; // 同 X（10 距离 0）、Y 远（90）
    let gained = kill_monster(&mut w, l, 600);
    assert_eq!(gained, 280, "B 仍计入 n=3");
    assert_eq!(w.entities.get(l).unwrap().exp, 280);
    assert_eq!(w.entities.get(a).unwrap().exp, 280);
    assert_eq!(
        w.entities.get(b).unwrap().exp,
        280,
        "oracle 漏比 CurrY ⇒ B 仍分到经验"
    );
}

/// `!Death` 门（`Base.cs:779`）：死亡成员被排除。B 死亡 ⇒ `n = 2`，
/// `dwExp = Round(600 * bonus[2]) = 780`，每人 390；B 得 0。
#[test]
fn group_exp_excludes_dead_member() {
    let (mut w, l, a, b) = setup_group();
    w.entities.get_mut(b).unwrap().death = true;
    let gained = kill_monster(&mut w, l, 600);
    assert_eq!(gained, 390);
    assert_eq!(w.entities.get(l).unwrap().exp, 390);
    assert_eq!(w.entities.get(a).unwrap().exp, 390);
    assert_eq!(w.entities.get(b).unwrap().exp, 0, "死亡成员不分经验");
}

/// `Envir == playObject.Envir` 门（`Base.cs:779`）：不同图成员被排除。B 在 map 1 ⇒ `n = 2`。
#[test]
fn group_exp_excludes_other_map_member() {
    let (mut w, l, a, b) = setup_group();
    w.entities.get_mut(b).unwrap().map_id = 1;
    let gained = kill_monster(&mut w, l, 600);
    assert_eq!(gained, 390);
    assert_eq!(w.entities.get(l).unwrap().exp, 390);
    assert_eq!(w.entities.get(b).unwrap().exp, 0, "不同图成员不分经验");
    assert_eq!(w.entities.get(a).unwrap().exp, 390);
}

/// `sumlv > 0 && n > 1` 为假（组内只有自己）⇒ 走 `WinExp(dwExp)`，`bonus` 不加成
/// （`Base.cs:806-810`）：600。
#[test]
fn single_member_group_gets_plain_exp() {
    let mut w = World::new(32, 32);
    for v in w.need_exps.iter_mut() {
        *v = 1_000_000;
    }
    let l = w.add_entity(Entity::new(1, "L", 10, 10));
    // 手造「只有自己」的组态（等价于 oracle `GroupMembers.Count == 1`）。
    {
        let e = w.entities.get_mut(l).unwrap();
        e.group_owner = l;
        e.group_members = vec![l];
    }
    let gained = kill_monster(&mut w, l, 600);
    assert_eq!(gained, 600, "n<=1 ⇒ 无 bonus");
    assert_eq!(w.entities.get(l).unwrap().exp, 600);
}

/// `exp_awards` 逐成员列出（宿主据此逐人出 `SM_WINEXP`），顺序 = 团长成员表序。
#[test]
fn group_exp_awards_list_members_in_order() {
    let (mut w, l, a, b) = setup_group();
    let mon = {
        let mut e = Entity::new(0, "chicken", 10, 9);
        e.race = ACTOR_RACE_MONSTER;
        e.mon_exp = 600;
        e.exp_hitter = Some(l);
        w.add_entity(e)
    };
    w.entities.get_mut(mon).unwrap().hp = 0;
    let rec = w.tick();
    assert_eq!(awards(&w, &rec), vec![l, a, b], "成员表序 L/A/B");
    assert!(rec.exp_awards.iter().all(|(_, amt, ..)| *amt == 280));
}

/// `HAM_GROUP`（`PlayObject.Attack.cs:667-677`）：同组玩家被 `IsGroupMember` 剔除；
/// 非成员 / 无组 / 非 Play 目标不受影响。
#[test]
fn ham_group_filter_matches_oracle() {
    let mut a = Entity::new(1, "A", 10, 10);
    a.attack_mode = ATTACK_MODE_HAM_GROUP;
    a.group_owner = 1; // A 自己是团长
    let t = Entity::new(2, "B", 11, 10);
    assert!(
        !is_attack_target_player_grouped(&a, &t, &[1, 2]),
        "同组玩家 ⇒ IsGroupMember 剔除"
    );
    assert!(
        is_attack_target_player_grouped(&a, &t, &[1]),
        "非成员（成员表不含 B）仍可打"
    );

    // `GroupOwner == 0` ⇒ `IsGroupMember` 第一步即 false ⇒ 成员表不生效。
    let mut solo = Entity::new(3, "C", 10, 10);
    solo.attack_mode = ATTACK_MODE_HAM_GROUP;
    assert!(is_attack_target_player_grouped(&solo, &t, &[1, 2]));

    // 非 Play 目标：Race 门先置 true，`target.Race == ActorRace.Play` 为假 ⇒ 不剔除。
    let mut mon = Entity::new(4, "m", 11, 10);
    mon.race = ACTOR_RACE_MONSTER;
    assert!(is_attack_target_player_grouped(&a, &mon, &[1, 2]));
    assert_eq!(t.race, ACTOR_RACE_PLAY);
}

/// 进程级：`World::attack_once` 对同组玩家不造成伤害（走 `attack_dir_grouped` 的成员表）。
///
/// 改坏必红：把 `attack_once` 里的 `group_members` 换成 `&[]` ⇒ `HAM_GROUP` 不再剔除，
/// B 掉血、本用例 `hp` 断言必红。
#[test]
fn world_attack_excludes_group_mate() {
    let mut w = World::new(32, 32);
    let a = w.add_entity({
        let mut e = Entity::new(1, "A", 10, 10);
        e.attack_mode = ATTACK_MODE_HAM_GROUP;
        e.dc = 0x0A14; // LoByte=20：若剔除失效，护甲 0 的 B 必掉血
        e
    });
    let b = w.add_entity(Entity::new(2, "B", 11, 10));
    {
        let ea = w.entities.get_mut(a).unwrap();
        ea.group_owner = a;
        ea.group_members = vec![a, b];
    }
    w.entities.get_mut(b).unwrap().group_owner = a;
    let hp_before = w.entities.get(b).unwrap().hp;
    w.attack_cmd(a, b);
    let rec = w.tick();
    assert_eq!(rec.attacks.len(), 1, "应有一次攻击结算: {rec:?}");
    assert_eq!(rec.attacks[0].2.damage, 0, "同组 ⇒ damage=0");
    assert!(!rec.attacks[0].2.hit, "同组 ⇒ 未命中");
    assert_eq!(w.entities.get(b).unwrap().hp, hp_before, "同组不得掉血");
}
