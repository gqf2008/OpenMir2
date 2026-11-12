//! hyper 服务面：路由、鉴权/限流/方法闸门、流式下载与审计日志。
//!
//! 请求处理顺序（顺序本身就是判据）：
//!
//! 1. **鉴权**（`Authorization: Bearer`，缺失/不符 ⇒ 403，**零触达**任何路由/文件系统）；
//! 2. **限流**（每 token 令牌桶，超限 ⇒ 429 + `Retry-After: 1`）；
//! 3. **方法白名单**（只 GET/HEAD，其余 ⇒ 405 + `Allow: GET, HEAD`）；
//! 4. 路由：`/res/index.json` 或 `/res/file/<path>`（其余 404）；
//! 5. 下载面再经 [`crate::pathmap`] 归一化/包含检查（403/404）与限额（413/416）。
//!
//! 审计日志红线：**只写 stderr**、一行一请求，字段是 method/path/status/bytes/dur_ms；
//! 绝不写任何请求头（含 `Authorization` 与 UA），path 里若意外出现 token 子串会被
//! [`redact_path`] 替换成 `<redacted>`。
//!
//! 连接级护栏：请求头读取有 `header_read_timeout`；**已建连接**另有 [`IdleTimeoutIo`]
//! 读写空闲超时——某方向连续 `idle_timeout` 无进展即关闭连接、释放并发名额
//! （慢读/不读响应的客户端不得无限期占坑；有进展会重置计时器，正常下载不受影响）。

use std::convert::Infallible;
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use futures_util::stream;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::{Bytes, Frame, Incoming};
use hyper::header::{
    HeaderValue, ACCEPT_RANGES, ALLOW, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE,
    RETRY_AFTER,
};
use hyper::service::Service;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio::time::Sleep;

use crate::auth;
use crate::config::ResConfig;
use crate::index::{self, HashCache};
use crate::pathmap::{self, ResolveError};
use crate::range::{self, Resolution};
use crate::ratelimit::RateLimiter;

/// 响应体类型（单请求装箱；错误是 IO 错误）。
pub type ResponseBody = UnsyncBoxBody<Bytes, io::Error>;

/// 流式下载的块大小（≤1 MiB 上限之内）。
pub const STREAM_CHUNK: usize = 64 * 1024;

/// 审计日志里 path 字段的最大字符数（超出截断并加省略号）。
const AUDIT_PATH_MAX: usize = 512;

/// 进程级共享状态。
pub struct AppState {
    /// 已校验的运行配置（token 的 Debug 是 `<redacted>`）。
    pub cfg: Arc<ResConfig>,
    /// 清单哈希缓存。
    pub cache: HashCache,
    /// 每 token 限流器（当前部署单 token ⇒ 一桶）。
    pub limiter: RateLimiter,
}

impl AppState {
    /// 由配置构造状态（清单缓存 TTL 与连接空闲超时都来自配置）。
    #[must_use]
    pub fn new(cfg: ResConfig) -> Self {
        let limiter = RateLimiter::new(cfg.rate_limit_burst, cfg.rate_limit_rps);
        let cache = HashCache::new(cfg.index_cache_ttl);
        Self {
            cfg: Arc::new(cfg),
            cache,
            limiter,
        }
    }
}

/// 每连接交给 hyper 的 `Service`（拥有状态 `Arc`，可跨任务移动）。
#[derive(Clone)]
pub struct AppService {
    state: Arc<AppState>,
}

impl AppService {
    /// 包装共享状态。
    #[must_use]
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl Service<Request<Incoming>> for AppService {
    type Response = Response<ResponseBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Infallible>> + Send>>;

    fn call(&self, request: Request<Incoming>) -> Self::Future {
        let state = Arc::clone(&self.state);
        Box::pin(async move { Ok(handle(state, request).await) })
    }
}

async fn handle(state: Arc<AppState>, request: Request<Incoming>) -> Response<ResponseBody> {
    let started = Instant::now();
    let method = request.method().clone();
    let raw_path = request.uri().path().to_string();
    let range_header = request.headers().get(RANGE).map(|value| {
        value
            .to_str()
            .map_or_else(|_| String::new(), ToString::to_string)
    });

    let response = if !auth::authorized(request.headers(), &state.cfg.token) {
        error_response(StatusCode::FORBIDDEN, "forbidden")
    } else if !state.limiter.allow() {
        let mut response = error_response(StatusCode::TOO_MANY_REQUESTS, "rate limited");
        response
            .headers_mut()
            .insert(RETRY_AFTER, HeaderValue::from_static("1"));
        response
    } else if method != Method::GET && method != Method::HEAD {
        let mut response = error_response(StatusCode::METHOD_NOT_ALLOWED, "method not allowed");
        response
            .headers_mut()
            .insert(ALLOW, HeaderValue::from_static("GET, HEAD"));
        response
    } else {
        route(
            Arc::clone(&state),
            &method,
            &raw_path,
            range_header.as_deref(),
        )
        .await
    };

    audit(&state.cfg.token, &method, &raw_path, &response, started);
    response
}

async fn route(
    state: Arc<AppState>,
    method: &Method,
    raw_path: &str,
    range_header: Option<&str>,
) -> Response<ResponseBody> {
    if raw_path == "/res/index.json" {
        return index_response(state, method).await;
    }
    if let Some(tail) = raw_path.strip_prefix("/res/file/") {
        return file_response(&state, method, tail, range_header).await;
    }
    error_response(StatusCode::NOT_FOUND, "not found")
}

async fn index_response(state: Arc<AppState>, method: &Method) -> Response<ResponseBody> {
    let roots = state.cfg.roots.clone();
    let allowed_exts = state.cfg.allowed_exts.clone();
    let cache = state.cache.clone();
    let entries = match tokio::task::spawn_blocking(move || {
        index::build_index(&roots, &allowed_exts, &cache)
    })
    .await
    {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("res-svr: 清单生成任务失败：{error}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal error");
        }
    };
    let files: Vec<serde_json::Value> = entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "path": entry.path,
                "size": entry.size,
                "sha256": entry.sha256,
            })
        })
        .collect();
    let payload = serde_json::json!({ "files": files });
    let Ok(body) = serde_json::to_vec(&payload) else {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal error");
    };
    response_with_body(StatusCode::OK, "application/json", body, method)
}

async fn file_response(
    state: &AppState,
    method: &Method,
    raw_tail: &str,
    range_header: Option<&str>,
) -> Response<ResponseBody> {
    let resolved = match pathmap::resolve(&state.cfg.roots, &state.cfg.allowed_exts, raw_tail) {
        Ok(resolved) => resolved,
        Err(ResolveError::Rejected(reason)) => {
            // reason 是静态文案，不含请求内容细节；path 由审计行单独记录（已脱敏）。
            eprintln!("res-svr: 路径拒绝（{reason}）");
            return error_response(StatusCode::FORBIDDEN, "forbidden");
        }
        Err(ResolveError::NotFound) => return error_response(StatusCode::NOT_FOUND, "not found"),
    };
    if resolved.size > state.cfg.max_file_size {
        eprintln!(
            "res-svr: 文件超过上限（{} > {}）：{}",
            resolved.size, state.cfg.max_file_size, resolved.display_path
        );
        return error_response(StatusCode::PAYLOAD_TOO_LARGE, "file too large");
    }
    match range::resolve(range::parse(range_header), resolved.size) {
        Resolution::Unsatisfiable => range_not_satisfiable(resolved.size),
        Resolution::Full => {
            file_response_full_or_partial(method, &resolved.path, 0, resolved.size, None).await
        }
        Resolution::Partial { start, end } => {
            let length = end - start + 1;
            let content_range = format!("bytes {start}-{end}/{}", resolved.size);
            file_response_full_or_partial(
                method,
                &resolved.path,
                start,
                length,
                Some(content_range),
            )
            .await
        }
    }
}

async fn file_response_full_or_partial(
    method: &Method,
    path: &Path,
    start: u64,
    length: u64,
    content_range: Option<String>,
) -> Response<ResponseBody> {
    let mut status = StatusCode::OK;
    if content_range.is_some() {
        status = StatusCode::PARTIAL_CONTENT;
    }
    let body = if method == Method::HEAD {
        full_body(Vec::new())
    } else {
        match open_segment(path, start).await {
            Ok(file) => file_body(file, length),
            Err(error) => {
                eprintln!("res-svr: 打开资产失败 {}：{error}", path.display());
                return if error.kind() == io::ErrorKind::NotFound {
                    error_response(StatusCode::NOT_FOUND, "not found")
                } else {
                    error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
                };
            }
        }
    };
    let mut response = Response::new(body);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(CONTENT_LENGTH, length_header(length));
    if let Some(content_range) = content_range {
        if let Ok(value) = HeaderValue::from_str(&content_range) {
            headers.insert(CONTENT_RANGE, value);
        }
    }
    response
}

fn range_not_satisfiable(total: u64) -> Response<ResponseBody> {
    let mut response = error_response(StatusCode::RANGE_NOT_SATISFIABLE, "range not satisfiable");
    if let Ok(value) = HeaderValue::from_str(&format!("bytes */{total}")) {
        response.headers_mut().insert(CONTENT_RANGE, value);
    }
    response
}

async fn open_segment(path: &Path, start: u64) -> io::Result<tokio::fs::File> {
    let mut file = tokio::fs::File::open(path).await?;
    if start > 0 {
        file.seek(io::SeekFrom::Start(start)).await?;
    }
    Ok(file)
}

fn file_body(file: tokio::fs::File, length: u64) -> ResponseBody {
    let stream = stream::unfold(
        (file, length),
        |(mut file, remaining): (tokio::fs::File, u64)| async move {
            if remaining == 0 {
                return None;
            }
            let want = usize::try_from(remaining.min(STREAM_CHUNK as u64))
                .unwrap_or(STREAM_CHUNK)
                .max(1);
            let mut buffer = vec![0u8; want];
            match file.read_exact(&mut buffer).await {
                Ok(_) => {
                    let consumed = u64::try_from(buffer.len()).unwrap_or(remaining);
                    Some((
                        Ok(Frame::data(Bytes::from(buffer))),
                        (file, remaining.saturating_sub(consumed)),
                    ))
                }
                Err(error) => Some((
                    Err(io::Error::new(
                        error.kind(),
                        format!("读取资产失败：{error}"),
                    )),
                    (file, 0),
                )),
            }
        },
    );
    StreamBody::new(stream).boxed_unsync()
}

fn response_with_body(
    status: StatusCode,
    content_type: &'static str,
    body: Vec<u8>,
    method: &Method,
) -> Response<ResponseBody> {
    let length = u64::try_from(body.len()).unwrap_or(u64::MAX);
    let payload = if method == Method::HEAD {
        full_body(Vec::new())
    } else {
        full_body(body)
    };
    let mut response = Response::new(payload);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(CONTENT_LENGTH, length_header(length));
    response
}

fn error_response(status: StatusCode, message: &'static str) -> Response<ResponseBody> {
    let mut body = Vec::with_capacity(message.len() + 1);
    body.extend_from_slice(message.as_bytes());
    body.push(b'\n');
    let length = u64::try_from(body.len()).unwrap_or(u64::MAX);
    let mut response = Response::new(full_body(body));
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    headers.insert(CONTENT_LENGTH, length_header(length));
    response
}

fn full_body(bytes: Vec<u8>) -> ResponseBody {
    Full::new(Bytes::from(bytes))
        .map_err(infallible_to_io)
        .boxed_unsync()
}

fn infallible_to_io(never: Infallible) -> io::Error {
    match never {}
}

fn length_header(value: u64) -> HeaderValue {
    HeaderValue::from_str(&value.to_string()).expect("十进制长度是合法头值")
}

fn audit(
    token: &str,
    method: &Method,
    raw_path: &str,
    response: &Response<ResponseBody>,
    started: Instant,
) {
    let status = response.status().as_u16();
    let bytes = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-");
    let path = redact_path(raw_path, token);
    eprintln!(
        "res-svr audit method={method} path={path} status={status} bytes={bytes} dur_ms={}",
        started.elapsed().as_millis()
    );
}

/// 审计脱敏：path 里若出现 token 子串（客户端误把 token 拼进 URL）⇒ 替换成 `<redacted>`；
/// 超长 path 截断（防止被打日志）。token 为空时不做替换。
fn redact_path(raw_path: &str, token: &str) -> String {
    let mut path = if token.is_empty() {
        raw_path.to_string()
    } else {
        raw_path.replace(token, "<redacted>")
    };
    if path.chars().count() > AUDIT_PATH_MAX {
        path = path.chars().take(AUDIT_PATH_MAX).collect::<String>();
        path.push('…');
    }
    path
}

/// 带读写空闲超时的连接 IO 包装。
///
/// 语义：**某方向连续 `idle` 无进展** ⇒ 该方向返回 [`io::ErrorKind::TimedOut`]，
/// hyper 结束连接、随连接任务释放并发许可。任一方向的读/写/刷出**有进展就重置**该方向的
/// 计时器 ⇒ 慢但持续在传输的大文件不会被误杀；只有彻底停住（慢读 DoS / 半开连接）才会超时。
struct IdleTimeoutIo<T> {
    inner: T,
    idle: Duration,
    read_deadline: Option<Pin<Box<Sleep>>>,
    write_deadline: Option<Pin<Box<Sleep>>>,
}

impl<T> IdleTimeoutIo<T> {
    fn new(inner: T, idle: Duration) -> Self {
        Self {
            inner,
            idle,
            read_deadline: None,
            write_deadline: None,
        }
    }

    /// 已武装则检查是否到期；未武装则此刻武装。到期 ⇒ `Ready(超时错误)`。
    fn poll_idle(
        deadline: &mut Option<Pin<Box<Sleep>>>,
        idle: Duration,
        cx: &mut Context<'_>,
    ) -> Poll<io::Error> {
        let sleep = deadline.get_or_insert_with(|| Box::pin(tokio::time::sleep(idle)));
        if sleep.as_mut().poll(cx).is_ready() {
            *deadline = None;
            eprintln!("res-svr: 连接读/写空闲超时（{idle:?} 无进展），关闭连接");
            return Poll::Ready(io::Error::new(io::ErrorKind::TimedOut, "连接读写空闲超时"));
        }
        Poll::Pending
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for IdleTimeoutIo<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(result) => {
                this.read_deadline = None;
                Poll::Ready(result)
            }
            Poll::Pending => match Self::poll_idle(&mut this.read_deadline, this.idle, cx) {
                Poll::Ready(error) => Poll::Ready(Err(error)),
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for IdleTimeoutIo<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_write(cx, buf) {
            Poll::Ready(result) => {
                this.write_deadline = None;
                Poll::Ready(result)
            }
            Poll::Pending => match Self::poll_idle(&mut this.write_deadline, this.idle, cx) {
                Poll::Ready(error) => Poll::Ready(Err(error)),
                Poll::Pending => Poll::Pending,
            },
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_flush(cx) {
            Poll::Ready(result) => {
                this.write_deadline = None;
                Poll::Ready(result)
            }
            Poll::Pending => match Self::poll_idle(&mut this.write_deadline, this.idle, cx) {
                Poll::Ready(error) => Poll::Ready(Err(error)),
                Poll::Pending => Poll::Pending,
            },
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_shutdown(cx) {
            Poll::Ready(result) => {
                this.write_deadline = None;
                Poll::Ready(result)
            }
            Poll::Pending => match Self::poll_idle(&mut this.write_deadline, this.idle, cx) {
                Poll::Ready(error) => Poll::Ready(Err(error)),
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

/// 起 HTTP 服务，直到进程结束（不返回除非监听失败）。
///
/// # Errors
/// 绑定监听地址失败（端口被占、权限不足）返回人读错误。
pub async fn serve(state: Arc<AppState>) -> Result<(), String> {
    let listener = TcpListener::bind(state.cfg.bind)
        .await
        .map_err(|error| format!("无法监听 {}：{error}", state.cfg.bind))?;
    let local = listener
        .local_addr()
        .map_err(|error| format!("读取监听地址失败：{error}"))?;
    // 启动行只暴露地址与护栏参数；token 不打印、不进日志。
    eprintln!(
        "res-svr: 监听 http://{local}/res（Bearer token 已配置；并发上限 {}，限流 {}/s 突发 {}，单文件上限 {} B，空闲超时 {} ms，清单缓存 TTL {} ms）",
        state.cfg.max_connections,
        state.cfg.rate_limit_rps,
        state.cfg.rate_limit_burst,
        state.cfg.max_file_size,
        state.cfg.idle_timeout.as_millis(),
        state.cfg.index_cache_ttl.as_millis()
    );
    let permits = Arc::new(Semaphore::new(state.cfg.max_connections));
    loop {
        let (stream, _peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                // accept 失败要有退避，绝不紧凑自旋。
                eprintln!("res-svr: accept 失败：{error}（100ms 后重试）");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
            // 并发闸门：不排队，直接回一个最小 429 并关闭。
            tokio::spawn(reject_connections_over_limit(stream));
            continue;
        };
        let service = AppService::new(Arc::clone(&state));
        let header_timeout = state.cfg.header_timeout;
        let idle_timeout = state.cfg.idle_timeout;
        tokio::spawn(async move {
            // 许可随连接生命周期持有（keep-alive 连接关闭前一直占名额；空闲超时会结束连接）。
            let _permit = permit;
            let io = TokioIo::new(IdleTimeoutIo::new(stream, idle_timeout));
            let mut builder = hyper::server::conn::http1::Builder::new();
            builder
                .timer(TokioTimer::new())
                .header_read_timeout(header_timeout);
            if let Err(error) = builder.serve_connection(io, service).await {
                // 客户端提前断开 / 请求头超时 / 读空闲超时都是常态，不视为服务端故障。
                eprintln!("res-svr: 连接结束：{error}");
            }
        });
    }
}

/// 并发超限的最小原始 HTTP 429（不经过 hyper：此时没有可用连接名额）。
async fn reject_connections_over_limit(mut stream: TcpStream) {
    const BODY: &str = "too many connections\n";
    let response = format!(
        "HTTP/1.1 429 Too Many Requests\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nRetry-After: 1\r\n\r\n{BODY}",
        BODY.len()
    );
    use tokio::io::AsyncWriteExt;
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
    eprintln!("res-svr audit method=- path=- status=429 bytes=- dur_ms=0");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_path_hides_token_substring() {
        assert_eq!(
            redact_path("/res/file/Data/SECRET-TOKEN.wzl", "SECRET-TOKEN"),
            "/res/file/Data/<redacted>.wzl"
        );
        assert_eq!(
            redact_path("/res/file/Data/a.wzl", "SECRET"),
            "/res/file/Data/a.wzl"
        );
        assert_eq!(redact_path("/res/x", ""), "/res/x");
        let long = format!("/res/file/{}", "a".repeat(AUDIT_PATH_MAX * 2));
        let redacted = redact_path(&long, "");
        assert_eq!(redacted.chars().count(), AUDIT_PATH_MAX + 1);
        assert!(redacted.ends_with('…'));
    }

    #[test]
    fn error_responses_have_length_and_content_type() {
        let response = error_response(StatusCode::FORBIDDEN, "forbidden");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            response
                .headers()
                .get(CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok()),
            Some("10")
        );
        assert_eq!(
            response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/plain; charset=utf-8")
        );
    }
}
