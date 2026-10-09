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
cmp('stats.load_failures', rust.stats.call_failures + rust.stats.include_loads * 0, cs.stats.load_failures);

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
console.log(diffs === 0 ? '== 全部一致 ==' : `== ${diffs} 项差异 ==`);
