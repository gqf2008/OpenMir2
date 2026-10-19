//! GameSvr 的可复用编码层（供 bin 与进程级集成测试共用）。
//!
//! 只暴露**无 I/O、无全局状态**的纯编码模块：`smframe`（GameSvr → 网关的 `SM_*` 入图帧编码，
//! 以及网关 `client_frame` 变换的参照实现）。`main.rs` 里的进程骨架（tokio/World/网关链路）
//! 仍留在 bin 里，不进 lib。
//!
//! 存在的理由：进程级用例（`tests/relay.rs`）要拿**生产路径**产出的 `GM_DATA` body 直接过
//! [`smframe::client_frame`] 与 M0 金标准逐字节对拍 —— 集成测试够不到 bin 的私有模块，
//! 所以把这一层提成 lib（见 `openmir2-m2-enter-world-frames` 的判据②/红检）。
#![allow(clippy::doc_markdown)]

pub mod ability;
pub mod attackwire;
pub mod bag;
pub mod chat;
pub mod contentload;
pub mod ctor_fold;
pub mod downlink;
pub mod feature;
pub mod ground;
pub mod guardlist;
pub mod healthspell;
pub mod inbound;
pub mod itemwire;
pub mod magics;
pub mod mapdoor;
pub mod merchant;
pub mod mongen;
pub mod monitems;
pub mod movewire;
pub mod msgqueue;
pub mod npc;
pub mod npcs;
pub mod party;
pub mod pickwire;
pub mod queryuser;
pub mod recover;
pub mod run_frames;
pub mod run_segments;
pub mod selloff;
pub mod sessionwire;
pub mod shop;
pub mod slave;
pub mod smframe;
pub mod smframe_ext;
pub mod stamina;
pub mod trade;
pub mod useitem;
