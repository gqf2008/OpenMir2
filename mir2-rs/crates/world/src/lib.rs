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
//! 2. **确定性**：虚拟时钟 + 固定遍历顺序，模拟逻辑不读墙钟、不用 HashMap 迭代序；
//! 3. **零并行依赖**：本 crate 不依赖任何线程/异步库（`tests/no_parallel.rs` 钉住）。

pub mod aoi;
pub mod combat;
pub mod entity;
pub mod map;
pub mod session;
pub mod world;

pub use aoi::{search_view_range, search_view_range_death, update_visible_gay, visible_snapshot};
pub use combat::{attack, AttackOutcome, CombatConfig};
pub use entity::{Entity, EntityStore, VisibleFlag, ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY};
pub use map::{CellAttribute, CellObject, CellType, MapCellInfo, MapGrid};
pub use session::{Session, SessionError, WorldStage};
pub use world::{Command, EntitySnapshot, TickRecord, World, TICK_INTERVAL_MS};
