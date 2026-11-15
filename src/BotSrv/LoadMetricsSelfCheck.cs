using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Security.Cryptography;
using System.Text;

namespace BotSrv
{
    /// <summary>
    /// 卡 `openmir2-botsrv-loginconns-buffer`：`LoadMetrics.WriteLoginConnsLocked` 复用缓冲改造的**无栈牙**。
    ///
    /// 入口：`BotSrv.exe --selftest-loadmetrics`（不启动服务/不连端口；PASS ⇒ exit 0，任一用例不符 ⇒ exit 1）。
    ///
    /// 背景：旧实现每次 flush 都 `new StringBuilder(count*160+64)` + `ToString()` + `File.WriteAllText`，
    /// 500 档下 ≈ 160KB + 234KB / 5s 的确定性 LOH 垃圾（`docs/botsrv-loh-attribution-20261115.md`）。
    /// 改造把文本/字节缓冲跨 flush 复用，**落盘内容必须逐字节不变**。
    ///
    /// 判据（每条都有牙）：
    ///   ① 空表 ⇒ 0 字节文件（复用缓冲 Clear 之后不得吐旧内容）；
    ///   ② canonical 500 条 + 1 个 null 槽 ⇒ 新实现与**旧实现逐字复刻**的产物 byte-for-byte 相同
    ///      （diff=0 由测试断言，不是人工比对），且 SHA256 等于改动前钉下的 golden；
    ///   ③ 同状态连写两次 ⇒ 文件不变（牙：漏 `Clear()` 会追加/重复）；
    ///   ④ 500 → 7 缩容 ⇒ 文件等于旧实现 7 条产物且**长度变短**（牙：不截断会残留旧尾）；
    ///   ⑤ 非 ASCII 键（中文 / 代理对 emoji / 引号 / 200 字符长键）⇒ 与原实现字节相同
    ///      （牙：UTF-8 无 BOM、无额外转义）；
    ///   ⑥ **跨 chunk 代理对（P1 复审回归）**：`StringBuilder(1)` + U+D83D + U+DE00 + 40 个 b
    ///      使 `GetChunks()` = [1,1,40]，高低代理被切口拆开 ⇒ 必须**不抛异常**且字节与旧实现
    ///      （`File.WriteAllText(text)`）一致（牙：退回「逐 chunk GetByteCount 定容」会在 flush 时抛
    ///      `ArgumentException: output byte buffer too small`；实测旧测量 measure=43 / actual=44）；
    ///   ⑦ 单次落盘分配量：`GC.GetAllocatedBytesForCurrentThread()` 实测，新 ≤ 旧 30% 且 ≤ 32KB
    ///      （稳态复用后只剩 FileStream 的小对象）；
    ///   ⑧ 接线：`Init()` + `Flush()` 真把 `login_conns.ndjson` 写成与旧实现相同的字节。
    ///
    /// 期望值说明：`CanonicalSha256` 是**改动前**旧实现（本文件 `WriteLegacy` 为逐字复刻）对
    /// `PopulateCanonical(500)` 的输出；任何让落盘字节漂移的改动都会同时打红 ②。
    /// </summary>
    internal static class LoadMetricsSelfCheck
    {
        // 改动前旧实现对 PopulateCanonical(500) 的输出指纹（由旧实现生成后钉死；见类注释）。
        private const string CanonicalSha256 = "69c46c69e82a61f0a8fc3550cf59cf38b8b8e47dd9a4199eb592aa87748ece11";
        private const int CanonicalCount = 500;

        public static int Run()
        {
            int failed = 0;
            void Check(string name, bool ok, string detail = "")
            {
                if (!ok)
                {
                    failed++;
                }
                Console.WriteLine($"LOADMETRICS {(ok ? "PASS" : "FAIL")} {name}{(detail.Length > 0 ? " " + detail : "")}");
            }

            string dir = Path.Combine(Path.GetTempPath(), "botsrv-loadmetrics-selftest-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(dir);
            string pathNew = Path.Combine(dir, "login_conns.new.ndjson");
            string pathLegacy = Path.Combine(dir, "login_conns.legacy.ndjson");
            try
            {
                LoadMetrics.SelfCheckSetLoginConnPath(pathNew);

                // ---------- ① 空表 ----------
                LoadMetrics.SelfCheckResetLoginConns();
                LoadMetrics.SelfCheckWriteLoginConns();
                WriteLegacy(pathLegacy);
                byte[] emptyNew = File.ReadAllBytes(pathNew);
                byte[] emptyLegacy = File.ReadAllBytes(pathLegacy);
                Check("empty-file-zero-bytes", emptyNew.Length == 0, $"bytes={emptyNew.Length}");
                Check("empty-bytes-identical", BytesEqual(emptyNew, emptyLegacy),
                    $"sha_new={Sha(emptyNew)} sha_legacy={Sha(emptyLegacy)}");

                // ---------- ② canonical 500（含 null 槽） ----------
                PopulateCanonical(CanonicalCount);
                LoadMetrics.SelfCheckWriteLoginConns();
                WriteLegacy(pathLegacy);
                byte[] canonicalNew = File.ReadAllBytes(pathNew);
                byte[] canonicalLegacy = File.ReadAllBytes(pathLegacy);
                Check("canonical500-byte-identical", BytesEqual(canonicalNew, canonicalLegacy),
                    $"bytes={canonicalNew.Length} sha_new={Sha(canonicalNew)} sha_legacy={Sha(canonicalLegacy)}");
                Check("canonical500-golden-sha256", Sha(canonicalNew) == CanonicalSha256,
                    $"sha={Sha(canonicalNew)} expected={CanonicalSha256}");

                // ---------- ③ 幂等 ----------
                LoadMetrics.SelfCheckWriteLoginConns();
                byte[] second = File.ReadAllBytes(pathNew);
                Check("repeat-write-idempotent", BytesEqual(canonicalNew, second),
                    $"len1={canonicalNew.Length} len2={second.Length} sha2={Sha(second)}");

                // ---------- ④ 缩容截断 ----------
                PopulateCanonical(7);
                LoadMetrics.SelfCheckWriteLoginConns();
                WriteLegacy(pathLegacy);
                byte[] shrunkNew = File.ReadAllBytes(pathNew);
                byte[] shrunkLegacy = File.ReadAllBytes(pathLegacy);
                Check("shrink-500-to-7-truncates",
                    BytesEqual(shrunkNew, shrunkLegacy) && shrunkNew.Length < canonicalNew.Length,
                    $"len_after={shrunkNew.Length} len_before={canonicalNew.Length}");

                // ---------- ⑤ 非 ASCII / 特殊键 ----------
                PopulateUnicode();
                LoadMetrics.SelfCheckWriteLoginConns();
                WriteLegacy(pathLegacy);
                byte[] uniNew = File.ReadAllBytes(pathNew);
                byte[] uniLegacy = File.ReadAllBytes(pathLegacy);
                Check("unicode-keys-byte-identical", BytesEqual(uniNew, uniLegacy),
                    $"bytes={uniNew.Length} sha_new={Sha(uniNew)} sha_legacy={Sha(uniLegacy)}");

                // ---------- ⑥ 跨 chunk 代理对（P1 复审回归） ----------
                // 生产路径用 GetMaxByteCount 上界定容、stateful GetBytes 编码；旧实现是整串
                // File.WriteAllText。本用例让 StringBuilder 的 GetChunks() 把代理对拆开。
                var tiny = new StringBuilder(1);
                tiny.Append((char)0xD83D);   // high surrogate
                tiny.Append((char)0xDE00);   // low surrogate
                tiny.Append('b', 40);
                var chunkLens = new List<int>();
                foreach (ReadOnlyMemory<char> chunk in tiny.GetChunks())
                {
                    chunkLens.Add(chunk.Length);
                }
                Check("cross-chunk-fixture-splits-pair",
                    chunkLens.Count >= 2 && chunkLens[0] == 1 && chunkLens[1] == 1,
                    $"chunks={string.Join(",", chunkLens)}");
                LoadMetrics.SelfCheckSetLoginConnBytesBufferSize(1);
                Check("cross-chunk-buffer-shrunk",
                    LoadMetrics.SelfCheckLoginConnBytesBufferLength == 1,
                    $"buffer={LoadMetrics.SelfCheckLoginConnBytesBufferLength}");
                string crossLegacyPath = Path.Combine(dir, "cross-chunk.legacy.txt");
                File.WriteAllText(crossLegacyPath, tiny.ToString(), new UTF8Encoding(false));
                byte[] crossLegacy = File.ReadAllBytes(crossLegacyPath);
                byte[] crossNew = null;
                Exception crossEx = null;
                try
                {
                    crossNew = LoadMetrics.SelfCheckEncodeText(tiny);
                }
                catch (Exception ex)
                {
                    crossEx = ex;
                }
                Check("cross-chunk-surrogate-no-throw", crossEx == null,
                    crossEx == null ? "" : crossEx.GetType().Name + ": " + crossEx.Message);
                Check("cross-chunk-buffer-grew",
                    crossEx == null && LoadMetrics.SelfCheckLoginConnBytesBufferLength >= crossNew.Length,
                    crossEx == null
                        ? $"buffer={LoadMetrics.SelfCheckLoginConnBytesBufferLength} bytes={crossNew.Length}"
                        : "exception");
                Check("cross-chunk-surrogate-bytes-equal", crossEx == null && BytesEqual(crossNew, crossLegacy),
                    crossEx == null
                        ? $"bytes={crossNew.Length} sha_new={Sha(crossNew)} sha_legacy={Sha(crossLegacy)}"
                        : "exception (see previous case)");

                // ---------- ⑦ 单次分配量 ----------
                PopulateCanonical(CanonicalCount);
                WriteLegacy(pathLegacy);                        // 预热（JIT）
                long l0 = GC.GetAllocatedBytesForCurrentThread();
                WriteLegacy(pathLegacy);
                long legacyAlloc = GC.GetAllocatedBytesForCurrentThread() - l0;
                LoadMetrics.SelfCheckWriteLoginConns();         // 预热：缓冲区按峰值增长一次
                LoadMetrics.SelfCheckWriteLoginConns();
                long n0 = GC.GetAllocatedBytesForCurrentThread();
                LoadMetrics.SelfCheckWriteLoginConns();
                long newAlloc = GC.GetAllocatedBytesForCurrentThread() - n0;
                double ratio = legacyAlloc > 0 ? (double)newAlloc / legacyAlloc : 1.0;
                Check("single-flush-alloc-reduced",
                    legacyAlloc > 0 && newAlloc <= 32768 && ratio <= 0.30,
                    $"before={legacyAlloc}B after={newAlloc}B reduction={(1.0 - ratio) * 100.0:F2}% n={CanonicalCount}");

                // ---------- ⑧ Flush 接线 ----------
                PopulateCanonical(CanonicalCount);
                string wireDir = Path.Combine(dir, "wire");
                LoadMetrics.Init(wireDir);
                LoadMetrics.Flush();
                LoadMetrics.Shutdown();
                string wirePath = Path.Combine(wireDir, "login_conns.ndjson");
                bool wireExists = File.Exists(wirePath);
                Check("flush-wiring-file-exists", wireExists, wirePath);
                if (wireExists)
                {
                    WriteLegacy(pathLegacy);
                    byte[] wire = File.ReadAllBytes(wirePath);
                    byte[] lg = File.ReadAllBytes(pathLegacy);
                    Check("flush-wiring-byte-identical", BytesEqual(wire, lg),
                        $"bytes={wire.Length} sha_wire={Sha(wire)} sha_legacy={Sha(lg)}");
                }
            }
            finally
            {
                try { Directory.Delete(dir, true); } catch { }
            }

            Console.WriteLine($"LOADMETRICS {(failed == 0 ? "ALL PASS" : "FAILED")} failures={failed}");
            return failed == 0 ? 0 : 1;
        }

        /// <summary>canonical 夹具：前 count 条有值 + 末尾一个 null 槽（旧实现必须跳过）。</summary>
        private static void PopulateCanonical(int count)
        {
            LoadMetrics.SelfCheckResetLoginConns();
            for (int i = 0; i < count; i++)
            {
                LoadMetrics.SelfCheckAddLoginConn(CanonicalKey(i), CanonicalConn(i));
            }
            LoadMetrics.SelfCheckAddLoginConn("null-slot", null);
        }

        private static string CanonicalKey(int i)
        {
            if (i == 3)
            {
                return "bot-quote\"key";               // 旧实现不转义键；两边必须同样不转义
            }
            return "bot-" + i.ToString("D4", CultureInfo.InvariantCulture);
        }

        private static LoadMetrics.LoginConn CanonicalConn(int i)
        {
            bool saw = (i % 3) == 0;
            return new LoadMetrics.LoginConn
            {
                ConnSeq = i,
                LoginTick = 1790000000L + i * 37L,
                LoginSends = 1 + (i % 2),
                RxChunks = i % 5,
                RxBytes = (long)i * 11L,
                Saw529 = saw,
                FirstRespIdent = saw ? OpenMir2.Messages.SM_PASSOK_SELECTSERVER : 0,
                FirstRespTick = saw ? 1790000100L + i : -1L,
                LastIdent = (i % 6) * 100,
                ResendSends = (i % 7) == 0 ? 1 : 0,
                ResendTick = (i % 7) == 0 ? 1790000200L + i : 0L,
            };
        }

        /// <summary>非 ASCII 键夹具：中文、代理对 emoji、引号、200 字符长键。</summary>
        private static void PopulateUnicode()
        {
            LoadMetrics.SelfCheckResetLoginConns();
            string[] keys =
            {
                "机器人-01",
                "bot-\U0001F600-emoji",
                "bot-\U0001F601\U0001F602-pair",
                "quote\"and\\back",
                new string('k', 200),
            };
            for (int i = 0; i < keys.Length; i++)
            {
                LoadMetrics.SelfCheckAddLoginConn(keys[i], CanonicalConn(i + 1));
            }
        }

        /// <summary>改动前 `WriteLoginConnsLocked` 的**逐字复刻**（只把输出路径参数化）。
        /// 等价性判据是「新实现 vs 这段旧代码」在同一字典枚举序上的字节比较。</summary>
        private static void WriteLegacy(string path)
        {
            Dictionary<string, LoadMetrics.LoginConn> conns = LoadMetrics.SelfCheckLoginConns;
            var sb = new StringBuilder(conns.Count * 160 + 64);
            foreach (KeyValuePair<string, LoadMetrics.LoginConn> kv in conns)
            {
                LoadMetrics.LoginConn c = kv.Value;
                if (c == null)
                {
                    continue;
                }
                sb.Append("{\"key\":\"").Append(kv.Key)
                  .Append("\",\"conn_seq\":").Append(c.ConnSeq)
                  .Append(",\"login_tick\":").Append(c.LoginTick)
                  .Append(",\"sends\":").Append(c.LoginSends)
                  .Append(",\"rx_chunks\":").Append(c.RxChunks)
                  .Append(",\"rx_bytes\":").Append(c.RxBytes)
                  .Append(",\"saw_529\":").Append(c.Saw529 ? "true" : "false")
                  .Append(",\"first_resp_ident\":").Append(c.FirstRespIdent)
                  .Append(",\"first_resp_tick\":").Append(c.FirstRespTick)
                  .Append(",\"last_ident\":").Append(c.LastIdent)
                  .Append(",\"resend_sends\":").Append(c.ResendSends)
                  .Append(",\"resend_tick\":").Append(c.ResendTick)
                  .Append("}\n");
            }
            File.WriteAllText(path, sb.ToString(), new UTF8Encoding(false));
        }

        private static bool BytesEqual(byte[] a, byte[] b)
        {
            if (ReferenceEquals(a, b))
            {
                return true;
            }
            if (a == null || b == null || a.Length != b.Length)
            {
                return false;
            }
            for (int i = 0; i < a.Length; i++)
            {
                if (a[i] != b[i])
                {
                    return false;
                }
            }
            return true;
        }

        private static string Sha(byte[] bytes)
        {
            using (var sha = SHA256.Create())
            {
                byte[] hash = sha.ComputeHash(bytes);
                var sb = new StringBuilder(hash.Length * 2);
                for (int i = 0; i < hash.Length; i++)
                {
                    sb.Append(hash[i].ToString("x2", CultureInfo.InvariantCulture));
                }
                return sb.ToString();
            }
        }
    }
}
