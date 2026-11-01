//! MCP 服务：六个工具（`gm_list` / `gm_help` / `conf_get` / `conf_keys` / `conf_set` / `gm_execute`）。
//!
//! 传输是**本地 stdio**，不监听任何端口。工具输出统一走 `structured_content`：
//! 成功 `{"ok":true,…}`；拒绝 `CallToolResult::structured_error({"ok":false,"code":…,"wrote":false})`。
//! 任何校验失败都返回结构化错误，不 panic。

use std::fs;
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

/// `gm_execute` 入参（本片只回 `not_wired`）。
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct GmExecuteReq {
    /// 现网命令名。
    pub name: String,
    /// 参数槽（本片不解参数）。
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
}

#[tool_router]
impl Mir2Mcp {
    /// 输出 GM 命令全量目录（185 条）。条数必须等于 `gm_registry.json` 的 `summary.table_total`。
    #[tool(
        description = "列出 GM 命令全量目录（现网名/族/权限/实现态/描述）。条数断言 = gm_registry.json 的 table_total（185）。",
        annotations(read_only_hint = true)
    )]
    fn gm_list(&self, Parameters(_): Parameters<NoArgs>) -> CallToolResult {
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
    fn conf_set(&self, Parameters(req): Parameters<ConfSetReq>) -> CallToolResult {
        match conf::conf_set(
            &self.config,
            &req.section,
            &req.key,
            &req.value,
            req.dry_run,
        ) {
            Ok(out) => {
                let mut value = serde_json::to_value(out).unwrap_or(Value::Null);
                value["ok"] = true.into();
                CallToolResult::structured(value)
            }
            Err(e) => self.conf_error(e),
        }
    }

    /// 本片不接线：返回结构化 `not_wired`（需要 game-svr admin 通道，留 slice-2）。
    #[tool(
        description = "本片未接线：返回结构化 not_wired（执行 GM 命令需要 game-svr admin 通道，留 slice-2），不会伪造执行。",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    fn gm_execute(&self, Parameters(req): Parameters<GmExecuteReq>) -> CallToolResult {
        let command = self.catalog().ok().and_then(|c| c.find(req.name.trim()));
        let mut value = json!({
            "ok": false,
            "status": "not_wired",
            "code": "not_wired",
            "message": "gm_execute 未接线：执行 GM 命令需要 game-svr admin 通道（slice-2），本片不伪造执行",
            "slice": "slice-2",
            "requires": "game-svr admin 通道",
            "command_found": command.is_some(),
            "params": req.params.unwrap_or_default(),
            "wrote": false,
        });
        if let Some(command) = command {
            value["command"] = entry_json(command);
        }
        CallToolResult::structured_error(value)
    }
}

#[tool_handler(
    instructions = "OpenMir2 GM/开区配置 MCP 服务：gm_list / gm_help 暴露 185 条 GM 命令目录；conf_get / conf_keys / conf_set 读写 M2GameSvr/Server.conf 的白名单键；gm_execute 在 slice-1 返回结构化 not_wired。纯本地 stdio，不监听端口。"
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
