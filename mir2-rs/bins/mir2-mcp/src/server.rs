//! MCP 服务：六个工具（`gm_list` / `gm_help` / `conf_get` / `conf_keys` / `conf_set` / `gm_execute`）。
//!
//! 传输是**本地 stdio**，不监听任何端口。`gm_execute` 经 game-svr 的**本机 admin 通道**
//! （逐行 JSON，缺省关闭）执行，执行核心在 game-svr 内（与客户端 `@` / 脚本 `GMEXECUTE`
//! 同一收口）。工具输出统一走 `structured_content`：
//! 成功 `{"ok":true,…}`；拒绝 `CallToolResult::structured_error({"ok":false,"code":…,"wrote":false})`。
//! 任何校验失败都返回结构化错误，不 panic。

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::catalog::{Catalog, GmEntry, Registry, REGISTRY_JSON};
use crate::conf::{self, AppConfig, ConfError, WHITELIST};

/// MCP 服务实例。
pub struct Mir2Mcp {
    config: AppConfig,
    catalog: Result<Arc<Catalog>, String>,
}

/// 工具处理器进入次数（进程级）。供端到端测试观察「鉴权失败零触达」（A4）：
/// 默认只自增；设置 `MIR2_MCP_TOOL_TRACE=1` 时每次进入向 stderr 打一行带序号的日志点。
static TOOL_INVOCATIONS: AtomicU64 = AtomicU64::new(0);

/// 记录一次工具处理器进入（测试接缝；不改变任何工具语义）。
fn trace_tool(name: &str) {
    let count = TOOL_INVOCATIONS.fetch_add(1, Ordering::Relaxed) + 1;
    if std::env::var_os("MIR2_MCP_TOOL_TRACE").is_some() {
        eprintln!("mir2-mcp-tool-invoked: {name} #{count}");
    }
}

/// 无参工具（MCP `arguments: {}`）。
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct NoArgs {}

/// `gm_help` 入参。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GmHelpReq {
    /// 现网命令名（`OrdinalIgnoreCase`，如 `Tingwa` / `清理垃圾`）。
    pub name: String,
}

/// `conf_get` 入参。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfGetReq {
    /// 段名（`OrdinalIgnoreCase`）。
    pub section: String,
    /// 键名（`OrdinalIgnoreCase`）。
    pub key: String,
}

/// `conf_set` 入参。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConfSetReq {
    /// 段名；白名单只认 `[Setup]`。
    pub section: String,
    /// 键名；白名单见 `conf_keys`。
    pub key: String,
    /// 新值（十进制整数；范围见 `conf_keys`）。
    pub value: String,
    /// true = 只预演、零写（不建备份、不落盘）。
    #[serde(default)]
    pub dry_run: bool,
}

/// `gm_execute` 入参（卡 `openmir2-mcp-gm-execute` 钉扎 `{chr, cmd}`）。
///
/// `name`/`params` 是 slice-1 的兼容字段：**只在 `cmd` 缺失时**合成命令行
/// `@<name> <params…>`，走与 `cmd` 完全相同的执行路径——不是第二套语义。
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct GmExecuteReq {
    /// 在线角色名（必填；game-svr 里按会话名查）。
    pub chr: Option<String>,
    /// 完整命令行（权威字段，`@` 开头），如 `@时间` / `@LoadAdmin`。
    pub cmd: Option<String>,
    /// 兼容 slice-1：命令名（仅当 `cmd` 缺失时用于合成命令行）。
    pub name: Option<String>,
    /// 兼容 slice-1：参数槽（仅当 `cmd` 缺失时拼到合成命令行尾部）。
    #[serde(default)]
    pub params: Option<Vec<String>>,
}

impl Mir2Mcp {
    /// 构造（启动时解析一次 185 条目录；解析失败不 panic，工具会回结构化错误）。
    #[must_use]
    pub fn new(config: AppConfig) -> Self {
        let catalog = Catalog::load().map(Arc::new);
        Self { config, catalog }
    }

    fn catalog(&self) -> Result<&Catalog, Value> {
        match &self.catalog {
            Ok(catalog) => Ok(catalog),
            Err(e) => Err(error_json("catalog_unavailable", e)),
        }
    }

    fn registry(&self) -> Result<(Registry, String), Value> {
        match &self.config.registry_path {
            Some(path) => {
                let text = fs::read_to_string(path).map_err(|e| {
                    error_json(
                        "registry_read_failed",
                        &format!("读 {} 失败：{e}", path.display()),
                    )
                })?;
                let registry =
                    Registry::parse(&text).map_err(|e| error_json("registry_unavailable", &e))?;
                Ok((registry, format!("file:{}", path.display())))
            }
            None => {
                let registry = Registry::parse(REGISTRY_JSON)
                    .map_err(|e| error_json("registry_unavailable", &e))?;
                Ok((
                    registry,
                    "embedded:tests/parity/coverage/gm_registry.json".to_string(),
                ))
            }
        }
    }

    fn conf_error(&self, error: ConfError) -> CallToolResult {
        CallToolResult::structured_error(error.to_json())
    }

    /// `gm_execute` 的目标地址：env `MIR2_MCP_ADMIN_ADDR` > `127.0.0.1:<[Admin] AdminPort>`。
    ///
    /// `AdminPort` 缺省 0（game-svr 不监听）⇒ 结构化 `admin_not_configured`。
    /// 解析结果必须落在**回环**（`crate::admin::resolve_loopback`），否则结构化
    /// `admin_addr_not_loopback`（客户端侧纵深防御，服务端 accept 层同样只收回环）。
    fn admin_addr(&self) -> Result<String, Value> {
        if let Some(addr) = self.config.admin_addr.as_deref() {
            return Ok(addr.to_string());
        }
        let port = conf::admin_port(&self.config).map_err(|e| e.to_json())?;
        if port == 0 {
            return Err(json!({
                "ok": false,
                "code": "admin_not_configured",
                "message": "gm_execute 需要 game-svr 本机 admin 通道：请在 M2GameSvr/Server.conf 写 [Admin] AdminPort=<端口>（缺省 0 = 不监听），或设置 env MIR2_MCP_ADMIN_ADDR",
                "wrote": false,
            }));
        }
        Ok(format!("127.0.0.1:{port}"))
    }
}

#[tool_router]
impl Mir2Mcp {
    /// 输出 GM 命令全量目录（185 条）。条数必须等于 `gm_registry.json` 的 `summary.table_total`。
    #[tool(
        description = "列出 GM 命令全量目录（现网名/族/权限/实现态/描述）。条数断言 = gm_registry.json 的 table_total（185）。",
        annotations(read_only_hint = true)
    )]
    fn gm_list(&self, Parameters(_): Parameters<NoArgs>) -> CallToolResult {
        trace_tool("gm_list");
        let catalog = match self.catalog() {
            Ok(c) => c,
            Err(e) => return CallToolResult::structured_error(e),
        };
        let (registry, source) = match self.registry() {
            Ok(r) => r,
            Err(e) => return CallToolResult::structured_error(e),
        };
        let total = catalog.entries.len() as u64;
        if total != registry.table_total {
            return CallToolResult::structured_error(json!({
                "ok": false,
                "code": "catalog_count_mismatch",
                "message": format!(
                    "目录条数 {total} ≠ gm_registry.json summary.table_total {}",
                    registry.table_total
                ),
                "catalog_total": total,
                "table_total": registry.table_total,
                "wrote": false,
            }));
        }
        let commands: Vec<Value> = catalog
            .entries
            .iter()
            .map(|entry| {
                let mut value = entry_json(entry);
                if let Some(detail) = registry.details.get(&entry.name) {
                    value["registry_verdict"] =
                        detail.get("verdict").cloned().unwrap_or(Value::Null);
                    value["registry"] = detail.clone();
                }
                value
            })
            .collect();
        CallToolResult::structured(json!({
            "ok": true,
            "source": source,
            "format": registry.format,
            "table_total": registry.table_total,
            "catalog_total": total,
            "summary": registry.summary,
            "commands": commands,
        }))
    }

    /// 单条命令详情（含族、权限、oracle 描述/帮助、registry 违例明细）。
    #[tool(
        description = "按现网名查一条 GM 命令的详情（族/权限/实现态/描述/帮助；registry 有该条时附违例明细）。名字 OrdinalIgnoreCase。",
        annotations(read_only_hint = true)
    )]
    fn gm_help(&self, Parameters(req): Parameters<GmHelpReq>) -> CallToolResult {
        trace_tool("gm_help");
        let catalog = match self.catalog() {
            Ok(c) => c,
            Err(e) => return CallToolResult::structured_error(e),
        };
        let Some(entry) = catalog.find(req.name.trim()) else {
            return CallToolResult::structured_error(json!({
                "ok": false,
                "code": "unknown_command",
                "message": format!("GM 命令目录里没有 `{}`（现网名 OrdinalIgnoreCase）", req.name),
                "wrote": false,
            }));
        };
        let mut value = entry_json(entry);
        value["ok"] = true.into();
        if let Ok((registry, source)) = self.registry() {
            value["registry_source"] = source.into();
            if let Some(detail) = registry.details.get(&entry.name) {
                value["registry_verdict"] = detail.get("verdict").cloned().unwrap_or(Value::Null);
                value["registry"] = detail.clone();
            }
        }
        CallToolResult::structured(value)
    }

    /// 读 `Server.conf` 的一个键（任意段/键，只读；大小写按 oracle `ConfigFile` 口径）。
    #[tool(
        description = "读 <root>/M2GameSvr/Server.conf 的一个键（只读；section/key 用 oracle ConfigFile 的 OrdinalIgnoreCase 口径）。",
        annotations(read_only_hint = true)
    )]
    fn conf_get(&self, Parameters(req): Parameters<ConfGetReq>) -> CallToolResult {
        trace_tool("conf_get");
        match conf::conf_get(&self.config, &req.section, &req.key) {
            Ok(read) => {
                let mut value = serde_json::to_value(read).unwrap_or(Value::Null);
                value["ok"] = true.into();
                CallToolResult::structured(value)
            }
            Err(e) => self.conf_error(e),
        }
    }

    /// 列出白名单键（当前值/缺省/范围/语义/是否活键）。
    #[tool(
        description = "列出白名单可写键（[Setup] GenMonRate / MonsterPowerRate / ExpRate）及当前值、缺省、允许范围、语义、是否活键。",
        annotations(read_only_hint = true)
    )]
    fn conf_keys(&self, Parameters(_): Parameters<NoArgs>) -> CallToolResult {
        trace_tool("conf_keys");
        let path = self.config.server_conf_path();
        if !path.exists() {
            return self.conf_error(ConfError::ConfMissing { path });
        }
        let mut keys = Vec::new();
        for white in WHITELIST {
            let read = match conf::conf_get(&self.config, white.section, white.key) {
                Ok(read) => read,
                Err(e) => return self.conf_error(e),
            };
            keys.push(json!({
                "section": white.section,
                "key": white.key,
                "live": white.live,
                "writable": white.live,
                "default": white.default,
                "min": white.min,
                "max": white.max,
                "current": read.value,
                "found": read.found,
                "line": read.line,
                "semantics": white.semantics,
            }));
        }
        CallToolResult::structured(json!({
            "ok": true,
            "path": path.display().to_string(),
            "keys": keys,
        }))
    }

    /// 白名单写（备份 → 原子替换 → 回读校验）；`dry_run` 零写。
    #[tool(
        description = "白名单写 Server.conf 的 [Setup] 键：写前备份(返回路径) → 临时文件+rename 原子替换 → 回读校验。非白名单/非法值/缺键一律拒绝且零写；dry_run=true 只预演零写。",
        annotations(
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn conf_set(&self, Parameters(req): Parameters<ConfSetReq>) -> CallToolResult {
        trace_tool("conf_set");
        // F3：conf_set 内含跨进程锁等待（有界 sleep）与进程探测，绝不能在 tokio worker 上
        // 同步执行——并发 conf_set 数接近 worker 数时会把其它会话（如 gm_list）饿住到秒级。
        // 整段操作移入 blocking 池：等待/探测阻塞的是 blocking 线程，runtime worker 始终可调度。
        let config = self.config.clone();
        let joined = tokio::task::spawn_blocking(move || {
            conf::conf_set(&config, &req.section, &req.key, &req.value, req.dry_run)
        })
        .await;
        match joined {
            Ok(Ok(out)) => {
                let mut value = serde_json::to_value(out).unwrap_or(Value::Null);
                value["ok"] = true.into();
                CallToolResult::structured(value)
            }
            Ok(Err(e)) => self.conf_error(e),
            Err(error) => CallToolResult::structured_error(json!({
                "ok": false,
                "code": "internal_error",
                "message": format!("conf_set 阻塞任务异常结束：{error}"),
                "wrote": false,
            })),
        }
    }

    /// 经 game-svr 本机 admin 通道执行一条 GM 命令（`{chr, cmd}`）。
    ///
    /// 执行核心在 game-svr（`gmcmd::execute` + 事件末 GM 收口），本工具只传命令并透传结果：
    /// 未配置/地址非回环/连不上/超时/未知角色/未知命令全部返回结构化错误，不伪造执行。
    ///
    /// 透传的 `frames` 是 game-svr 的**直接应答帧数**（`SysMsg` + 未知命令回落 + 直发帧），
    /// **不含**事件末 `command_event_tail_gm_drain` 落地的副作用广播帧（与 `AdminGmReply` 一致）。
    #[tool(
        description = "经 game-svr 本机 admin 通道执行 GM 命令：入参 {chr, cmd}（cmd 为完整 @ 命令行，如 \"@时间\"）。返回 game-svr 的 {ok,error,frames,note}（frames = 直接应答帧数，不含事件末副作用广播帧）；未配置通道/地址非回环/连不上/超时/未知角色/未知命令均为结构化错误，不会伪造执行。",
        annotations(destructive_hint = true, open_world_hint = true)
    )]
    fn gm_execute(&self, Parameters(req): Parameters<GmExecuteReq>) -> CallToolResult {
        trace_tool("gm_execute");
        // ① `chr` 必填。
        let Some(chr) = req.chr.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
            return CallToolResult::structured_error(json!({
                "ok": false,
                "code": "missing_chr",
                "message": "gm_execute 需要 {chr, cmd}：chr = 在线角色名（game-svr 会话名）",
                "wrote": false,
            }));
        };
        // ② 命令行：`cmd` 权威；`name`/`params` 仅作兼容合成，走同一条执行路径。
        let cmd = match req.cmd.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(cmd) => cmd.to_string(),
            None => {
                let Some(name) = req.name.as_deref().map(str::trim).filter(|s| !s.is_empty())
                else {
                    return CallToolResult::structured_error(json!({
                        "ok": false,
                        "code": "missing_cmd",
                        "message": "gm_execute 需要 {chr, cmd}（cmd 为完整 @ 命令行）",
                        "wrote": false,
                    }));
                };
                let mut line = format!("@{name}");
                if let Some(params) = &req.params {
                    for param in params {
                        line.push(' ');
                        line.push_str(param);
                    }
                }
                line
            }
        };
        if !cmd.starts_with('@') {
            return CallToolResult::structured_error(json!({
                "ok": false,
                "code": "invalid_command",
                "message": format!("`cmd` 必须是 `@` 开头的完整命令行：{cmd:?}"),
                "chr": chr,
                "cmd": cmd,
                "wrote": false,
            }));
        }
        // ③ 目标地址（未配置 ⇒ 结构化错误）。
        let addr = match self.admin_addr() {
            Ok(addr) => addr,
            Err(value) => return CallToolResult::structured_error(value),
        };
        // ④ 投递 + 透传 game-svr 的结构化结果。
        match crate::admin::execute(&addr, chr, &cmd) {
            Ok(response) => {
                let ok = response["ok"].as_bool().unwrap_or(false);
                let mut out = json!({
                    "ok": ok,
                    "chr": chr,
                    "cmd": cmd,
                    "admin_addr": addr,
                    "frames": response.get("frames").cloned().unwrap_or_else(|| json!(0)),
                    "note": response.get("note").cloned().unwrap_or(Value::Null),
                    "wrote": false,
                });
                if let Some(error) = response.get("error") {
                    out["error"] = error.clone();
                }
                if ok {
                    CallToolResult::structured(out)
                } else {
                    let code = out["error"]
                        .as_str()
                        .and_then(|e| e.split(':').next())
                        .unwrap_or("command_failed");
                    out["code"] = json!(code);
                    CallToolResult::structured_error(out)
                }
            }
            Err(failure) => CallToolResult::structured_error(failure.to_json(&addr, chr, &cmd)),
        }
    }
}

#[tool_handler(
    instructions = "OpenMir2 GM/开区配置 MCP 服务：gm_list / gm_help 暴露 185 条 GM 命令目录；conf_get / conf_keys / conf_set 读写 M2GameSvr/Server.conf 的白名单键；gm_execute 经 game-svr 本机 admin 通道（[Admin] AdminPort，缺省关闭）执行 GM 命令并透传结构化结果。纯本地 stdio，不监听端口。"
)]
impl ServerHandler for Mir2Mcp {}

fn entry_json(entry: &GmEntry) -> Value {
    json!({
        "name": entry.name,
        "family": entry.family,
        "permission_min": entry.perm_min,
        "params": entry.params,
        "implemented": entry.implemented,
        "imp": entry.imp,
        "handler_class": entry.handler_class,
        "factory_name": entry.factory_name,
        "effect": entry.effect,
        "frames": entry.frames,
        "desc": entry.desc,
        "help": entry.help,
    })
}

fn error_json(code: &str, message: &str) -> Value {
    json!({
        "ok": false,
        "code": code,
        "message": message,
        "wrote": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("mir2-mcp-server-test-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(path.join("M2GameSvr")).expect("mkdir");
            fs::write(
                path.join("M2GameSvr").join("Server.conf"),
                b"\xEF\xBB\xBF[Setup]\r\nMonsterPowerRate=10\r\nGenMonRate=10\r\n",
            )
            .expect("写夹具");
            Self { path }
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    }

    /// F3：N = available_parallelism 个并发 `conf_set`（全部阻塞在跨进程锁等待）时，
    /// 另一会话的 `gm_list` 不得出现秒级饥饿。
    ///
    /// 修复前（同步处理器直接占用 tokio worker）：N 个 conf_set 占满全部 worker，gm_list
    /// 排队到某个 conf_set 5s 超时后才执行（实测秒级）。修复后（spawn_blocking）：worker
    /// 不被阻塞，gm_list 为毫秒级。阈值 1000ms 区分两态（不在秒级）。
    #[test]
    fn concurrent_conf_set_does_not_starve_other_tool_calls() {
        let workers = std::thread::available_parallelism()
            .map_or(4, std::num::NonZeroUsize::get)
            .clamp(2, 16);
        let root = TempRoot::new("starvation");
        // 活锁（pid = 本进程、年龄未超 stale）⇒ 每个 conf_set 都等待到默认 5s timeout。
        fs::write(
            root.path.join("M2GameSvr").join("Server.conf.lock"),
            format!(
                "pid={}\nacq_ms={}\nnonce=server-test-hold\n",
                std::process::id(),
                now_ms()
            ),
        )
        .expect("写活锁");

        let config = AppConfig {
            root: root.path.clone(),
            registry_path: None,
            admin_addr: None,
        };
        let mcp = Arc::new(Mir2Mcp::new(config));
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(workers)
            .enable_all()
            .build()
            .expect("multi-thread runtime");

        let started = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::with_capacity(workers);
        for index in 0..workers {
            let mcp = Arc::clone(&mcp);
            let started = Arc::clone(&started);
            handles.push(rt.handle().spawn(async move {
                started.fetch_add(1, Ordering::SeqCst);
                mcp.conf_set(Parameters(ConfSetReq {
                    section: "Setup".to_string(),
                    key: "GenMonRate".to_string(),
                    value: (100 + index).to_string(),
                    dry_run: false,
                }))
                .await
            }));
        }

        // 不用 tokio 定时器同步：N 个 worker 全被阻塞时定时器也可能不推进。
        // 在外部线程等计数器到 N，再留 100ms 让它们真正进入锁等待。
        let deadline = Instant::now() + Duration::from_secs(10);
        while started.load(Ordering::SeqCst) < workers {
            assert!(Instant::now() < deadline, "N 个 conf_set 会话未全部启动");
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(100));

        // 「另一会话」：队列时刻在外部取，任务一旦被 worker 调度立即测 end-to-end 延迟。
        let queued_at = Instant::now();
        let probe_mcp = Arc::clone(&mcp);
        let probe = rt.handle().spawn(async move {
            let out = probe_mcp.gm_list(Parameters(NoArgs {}));
            (queued_at.elapsed(), out.is_error == Some(false))
        });
        let (gm_latency, gm_ok) = rt.block_on(probe).expect("gm_list task");

        // 收尾：等待 N 个 conf_set 超时返回（默认 5s），并确认它们是有界失败而非 panic。
        for handle in handles {
            let out = rt.block_on(handle).expect("conf_set task");
            assert_eq!(
                out.is_error,
                Some(true),
                "活锁场景 conf_set 必须是结构化错误"
            );
        }

        eprintln!(
            "F3: workers={workers}, gm_list latency under saturated conf_set = {gm_latency:?}"
        );
        assert!(gm_ok, "gm_list 必须成功");
        assert!(
            gm_latency < Duration::from_millis(1000),
            "N=worker 并发 conf_set 时 gm_list 被饿住：{gm_latency:?}（基线应为毫秒级）"
        );
    }
}
