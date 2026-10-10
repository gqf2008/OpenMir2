// 对拍 diff：比较 Rust 侧与 C# 侧 parse-stats JSON。
// 用法: node diff-parity.js <rust.json> <cs.json>
const fs = require('fs');
const rust = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const cs = JSON.parse(fs.readFileSync(process.argv[3], 'utf8'));

let diffs = 0;
function cmp(label, a, b) {
  const same = a === b;
  if (!same) diffs++;
  console.log(`${same ? 'OK  ' : 'DIFF'} ${label}: rust=${a} cs=${b}`);
}

cmp('files', rust.files, cs.files);
cmp('loaded', rust.loaded, cs.loaded);
cmp('missing', rust.missing, cs.missing);
cmp('panics', rust.panics, cs.panics);
for (const k of ['scripts', 'records', 'procedures', 'conditions', 'actions', 'else_actions', 'goods', 'parse_errors']) {
  cmp(`stats.${k}`, rust.stats[k], cs.stats[k]);
}
cmp('stats.load_failures', rust.stats.call_failures + rust.stats.include_failures, cs.stats.load_failures);

// 错误逐条对拍：Rust {file, line} vs C# {file, message:"脚本错误: <line> 第:<i> 行: <path>"}
const rustErrs = rust.errors
  .filter(e => e.kind === 'ScriptError')
  .map(e => `${e.file}@@${e.line}`)
  .sort();
const csErrs = cs.errors.map(e => {
  const m = e.message.match(/^脚本错误: (.*) 第:\d+ 行: .*$/);
  return `${e.file}@@${m ? m[1] : e.message}`;
}).sort();
console.log(`错误条数: rust=${rustErrs.length} cs=${csErrs.length}`);
const rsSet = new Set(rustErrs), csSet = new Set(csErrs);
const onlyRust = rustErrs.filter(x => !csSet.has(x));
const onlyCs = csErrs.filter(x => !rsSet.has(x));
// 多重集差异（同一文件同一行可能多条）
function multisetDiff(a, b) {
  const cnt = new Map();
  for (const x of b) cnt.set(x, (cnt.get(x) || 0) + 1);
  const out = [];
  for (const x of a) {
    const c = cnt.get(x) || 0;
    if (c > 0) cnt.set(x, c - 1); else out.push(x);
  }
  return out;
}
const mdR = multisetDiff(rustErrs, csErrs);
const mdC = multisetDiff(csErrs, rustErrs);
console.log(`仅Rust: ${mdR.length} 仅C#: ${mdC.length} (集合差: ${onlyRust.length}/${onlyCs.length})`);
for (const x of mdR.slice(0, 10)) console.log('  R>: ' + x);
for (const x of mdC.slice(0, 10)) console.log('  C>: ' + x);
if (mdR.length || mdC.length) diffs++;

// 逐文件结构摘要对拍（F1）：哈希 + 计数必须逐文件相同；同时逐条比对文件集
const rd = rust.structure_digest || {};
const cd = cs.structure_digest || {};
const rKeys = Object.keys(rd).sort();
const cKeys = Object.keys(cd).sort();
const onlyR = rKeys.filter(k => !(k in cd));
const onlyC = cKeys.filter(k => !(k in rd));
console.log(`结构摘要: rust=${rKeys.length} 文件, cs=${cKeys.length} 文件`);
if (onlyR.length || onlyC.length) {
  diffs++;
  console.log(`DIFF 文件集: 仅Rust ${onlyR.length} (${onlyR.slice(0,5).join(', ')}), 仅C# ${onlyC.length} (${onlyC.slice(0,5).join(', ')})`);
}
let mismatch = [];
for (const k of rKeys) {
  if (!(k in cd)) continue;
  if (rd[k].hash !== cd[k].hash) mismatch.push(k);
}
if (mismatch.length) {
  diffs++;
  console.log(`DIFF 结构摘要不一致: ${mismatch.length} 个文件`);
  for (const k of mismatch.slice(0, 10)) {
    console.log(`  ${k}: rust=${JSON.stringify(rd[k])} cs=${JSON.stringify(cd[k])}`);
  }
} else {
  console.log(`OK   结构摘要逐文件一致（${rKeys.length} 个文件：label/cmd_code/参数/宏展开后内容全部相同）`);
}

// 逐文件原文摘要对拍（覆盖解码差异本身）
const rt = rust.text_digest || {};
const ct = cs.text_digest || {};
const rtk = Object.keys(rt).sort();
let tmis = [];
for (const k of rtk) {
  if (!(k in ct)) { tmis.push(`${k}(仅Rust)`); continue; }
  if (rt[k] !== ct[k]) tmis.push(k);
}
for (const k of Object.keys(ct).sort()) if (!(k in rt)) tmis.push(`${k}(仅C#)`);
if (tmis.length) { diffs++; console.log(`DIFF 原文摘要不一致: ${tmis.length} 个文件 -> ${tmis.slice(0,10).join(', ')}`); }
else console.log(`OK   原文摘要逐文件一致（${rtk.length} 个文件：解码结果完全相同）`);

console.log(diffs === 0 ? '== 全部一致 ==' : `== ${diffs} 项差异 ==`);
// 退出码：有差异 → 非零（供 CI/脚本判断，勿只看文本）
process.exitCode = diffs === 0 ? 0 : 1;
