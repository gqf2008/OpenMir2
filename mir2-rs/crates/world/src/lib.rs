//! M2 世界模拟骨架（单线程 tick + 实体存储 + 地图/AOI + 会话进出）。
//!
//! 行为基准：`src/GameSvr/Word/WorldServer.cs`（`ProcessHumans` 的 200ms 节拍）与
//! `src/M2Server/Maps/Envirnoment.cs`、`src/M2Server/Actor/BaseObject.ViewRange.cs`。
//!
//! 范围（明确不做）：M1 的边界服务（LoginGate/SelGate/LoginSrv/DBSrv 一律不在此 crate）；
//! 协议编解码见 `crates/protocol`（A 线）。
//!
//! 三条纪律：
//! 1. **单线程串行**：`World::tick` 每次校验线程 id（见 `world.rs` 的测试）；
//! 2. **确定性**：虚拟时钟 + 固定遍历顺序（S1 起遍历顺序来自 `mir2-world-bevy` 的显式槽位索引
//!    `EcsStore`），模拟逻辑不读墙钟、不用 HashMap 迭代序；
//! 3. **零并行依赖**：本 crate 只依赖 ECS 内核（`mir2-world-bevy`，单线程 executor）与
//!    数据/公式 crate，不引入任何线程/异步运行时（`tests/no_parallel.rs` 钉住）。

pub mod action;
pub mod aoi;
pub mod bevy_bridge;
pub mod buff;
pub mod castle;
pub mod castle_tax;
pub mod combat;
pub mod ctor_fold;
pub mod entity;
pub mod guild;
pub mod magic;
pub mod magic_effect;
pub mod map;
pub mod merchant;
pub mod monai;
pub mod mongen;
pub mod npc;
pub mod party;
pub mod run_gap;
pub mod session;
pub mod slave;
pub mod space_move;
pub mod special_monster;
pub mod world;

pub use action::{ButchEvent, PushEvent, RushEvent, RushFrame};
pub use aoi::{
    search_view_range, search_view_range_death, search_view_range_with_updates,
    update_monster_visible, update_visible_gay, visible_snapshot, SearchViewRangeOutcome,
};
pub use bevy_bridge::{collect_tick_order_system, snapshot_from_world};
pub use buff::{
    damage_health, get_char_status, ActorBuffData, BuffSysMsg, BuffSystem, BuffTickOutcome,
    BuffType, BUFF_DESTROY_TICK_MS, BUFF_EPOCH_MS, BUFF_PASSES_PER_TICK, BUFF_PASS_INTERVAL_MS,
    BUFF_STATE_TICK_GATE_MS, BUFF_STATE_TICK_MAX, MAX_STATUS_ATTRIBUTE,
};
pub use castle::{
    apply_structure_events, castle_official_action, compare_lstr, door_map_xy_flag_ops,
    door_state_text, hire_substring, hutil32_round, official_repair_door, official_repair_wall,
    AttackerInfo, CastleDate, CastleDefenseSpecs, CastleEvent, CastleInitializeEnv, CastleNow,
    CastleOfficialAction, CastleSiegeConfig, CastleUnit, DefenseCreature, DefenseRole,
    DefenseSpawn, DefenseUnitSpec, GuildId, HireSubstringError, OfficialRepairResult,
    StructureEvent, UserCastle, CLOSE_MAIN_DOOR, DOOR_STATE_CLOSED, DOOR_STATE_DESTROYED,
    DOOR_STATE_OPENED, HIRE_ARCHER_NOW, HIRE_GUARD_NOW, MAX_CASTLE_ARCHER, MAX_CASTLE_GUARD,
    OFFICIAL_REPAIR_NOT_ENOUGH_GOLD_MSG, OFFICIAL_REPAIR_NOT_NEEDED_MSG, OFFICIAL_REPAIR_OK_MSG,
    OPEN_MAIN_DOOR, REPAIR_DOOR_NOW, REPAIR_WALL_NOW_1, REPAIR_WALL_NOW_2, REPAIR_WALL_NOW_3,
    SS_204, SS_212, STRUCTURE_REPAIR_DELAY_MS,
};
pub use castle_tax::{
    merchant_tax_amount, merchant_tax_target, CastleManager, CastleTaxConfig, IncRateGoldOutcome,
    MerchantTaxTarget, CASTLE_GOLD_SAVE_INTERVAL_MS,
};
pub use combat::{
    attack, attack_dir, attack_dir_grouped, attack_success_weapon_damage, attack_with,
    attack_with_grouped, get_attack_power_hit, is_attack_target, is_attack_target_base,
    is_attack_target_player, is_attack_target_player_grouped, is_proper_target,
    is_proper_target_grouped, is_proper_target_grouped_direct, is_protect_target, recalc_hit_speed,
    win_exp, AttackHitState, AttackOutcome, AttackPowerMode, CombatConfig, HitSpeedInput,
    HitSpeedOutcome, ACTOR_RACE_ANIMAL, ACTOR_RACE_NPC, ACTOR_RACE_PEACE_NPC, ATTACK_MODE_HAM_ALL,
    ATTACK_MODE_HAM_DEAR, ATTACK_MODE_HAM_GROUP, ATTACK_MODE_HAM_GUILD, ATTACK_MODE_HAM_MASTER,
    ATTACK_MODE_HAM_PEACE, ATTACK_MODE_HAM_PKATTACK,
};
pub use ctor_fold::{
    CtorFoldEntry, CtorFoldLedger, CtorFoldPlan, CtorFoldSite, CTOR_FOLD_BARRIER_A_DRAWS,
    CTOR_FOLD_BARRIER_A_MOD, CTOR_FOLD_DRAWS_PER_SITE, CTOR_FOLD_DRAW_KIND,
};
pub use entity::{
    BaseAnimalCtorDraws, Entity, EntityStore, MonsterCtorDraws, VisibleFlag, ACTOR_RACE_MONSTER,
    ACTOR_RACE_PLAY, BASE_ANIMAL_CTOR_DRAW_MODS,
};
pub use guild::{
    check_guild_name, AddWarGuildOutcome, GuildConfig, GuildManager, GuildRank, GuildState,
    GuildWarEnded, GuildWarEvent, GuildWarOutcome, GuildWarRequest, WarGuild,
};
pub use magic::{
    can_read_book, get_power, get_power13, get_rpow, get_spell_point, magic_attack_interval, mpow,
};
pub use map::{
    CellAttribute, CellObject, CellType, FloorItemKind, FloorItemMeta, MapCellInfo, MapGrid,
};
pub use merchant::{
    MerchantFlags, MerchantGoods, MerchantSpawn, MerchantState, ACTOR_RACE_MERCHANT,
    MERCHANT_RUN_TIME,
};
pub use monai::{
    animal_struck, attack_target, big_heart_attack_target, can_move, del_target_creature,
    dir_delta, get_attack_dir, goto_target_xy, is_active_seek_race, is_animal_race, is_guard_race,
    make_long, monster_ai_tick, monster_class_run, monster_run, search_target, set_target_creature,
    think, turn_to, walk_to, wondering, ClassRunBase, MonsterRefMsg,
};
pub use mongen::{
    gen_mod_count, get_monsters_zen_time, initialize_monster, monster_death_gate,
    monster_due_for_ghost, monster_due_for_revive, monster_ghost_expired, process_cert_lists,
    regen_gate, regen_monsters, should_regen, step_threads, CertListRecord, MapCanCreate,
    MonGenConfig, MonGenEntry, MonGenState, MonGenThread, MonsterCreator, ReviveFrame,
    ReviveOutcome, TickClockFn, MON_GEN_THREAD_MON_LIMIT,
};
pub use npc::{NpcSpawn, NpcState, NPC_RUN_DELAY_MAX, NPC_SEARCH_DRAW_MAX};
pub use party::{
    PartyConfig, PartyEvent, PARTY_CANCEL_GROUP_MSG, PARTY_EXIT_GROUP_MSG, PARTY_EXP_BONUS,
    PARTY_GROUP_MAX, PARTY_GROUP_MEMBERS_MAX_DEFAULT, PARTY_GROUP_MSG_B_COLOR_DEFAULT,
    PARTY_GROUP_MSG_F_COLOR_DEFAULT, PARTY_GROUP_MSG_PREFIX_DEFAULT, PARTY_JOIN_GROUP_MSG_DEFAULT,
    PARTY_LEADER_CLOSE_MSG,
};
pub use session::{send_ref_msg_targets_at, Session, SessionError, WorldStage, SEND_REF_MSG_RANGE};
pub use slave::{
    check_server_make_slave, is_slave, mag_make_angel_slave_plan, mag_make_sinsu_slave_plan,
    mag_make_slave_plan, make_slave_gate, recall_slave_target, space_move_get_rand_xy, spawn_clone,
    summon_slave, CloneSpawn, MakeSlavePlan, SlaveSpawner, SummonOutcome, CLONE_NAME,
    PLAYER_VIEW_RANGE, SKILL_46, SKILL_ANGEL, SKILL_SINSU, SKILL_SKELLETON, SKILL_TAMMING,
};
pub use special_monster::{
    adopt_special_child, apply_ctor_fields, clone_operate, operate_special, special_monster_run,
    special_spawn_request, SpecialKind, SpecialRefMsg, SpecialSpawnOutcome, SpecialSpawnRequest,
    SpecialSpawner, CHILD_LIMIT, RACE_BEE_QUEEN, RACE_CLONE_MONSTER, RACE_SPIDER_HOUSE,
    RM_MAGSTRUCK, RM_MAGSTRUCK_MINE, RM_SPELL, RM_STRUCK, RM_ZEN_BEE, ZEN_BEE_DELAY_MS,
};
pub use world::{
    Command, DigOutEvent, EntitySnapshot, ExtraAbilEventKind, ExtraAbilExpiry, FireBurnEvent,
    FireWallOutcome, FloorItemView, PassThroughHit, PassThroughOutcome, RunStateExpiry,
    SpaceMoveCastOutcome, TickRecord, UEnhancerOutcome, World, TICK_INTERVAL_MS,
};
