//! res-svr 库：资产补齐下载服务的全部可判据逻辑。
//!
//! 二进制入口在 `main.rs`（薄壳：装配配置 → [`http::serve`]）。模块边界：
//!
//! * [`config`]：INI 文件 / 环境变量 / 命令行的合并与校验（token 不进 Debug、不进 argv）；
//! * [`auth`]：`Authorization: Bearer` + 常量时间比较；
//! * [`pathmap`]：请求路径的百分号解码、归一化、前缀映射与根内包含检查（安全承重）；
//! * [`index`]：清单遍历 + `sha256` 按 `(size, mtime)` + `index_cache_ttl_ms` 缓存；
//! * [`range`]：单段 `Range` 解析与饱和度计算；
//! * [`ratelimit`]：令牌桶限流（每 token 一桶；当前部署单 token ⇒ 等价全局一桶）；
//! * [`http`]：hyper 服务、路由、限额闸门与审计日志。

pub mod auth;
pub mod config;
pub mod http;
pub mod index;
pub mod pathmap;
pub mod range;
pub mod ratelimit;

#[cfg(test)]
mod testutil;
