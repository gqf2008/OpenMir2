import json, sys
d = json.load(open('target/parse-stats-rust.json', encoding='utf-8'))
files = sorted({e['file'] for e in d['errors']})
print(len(files), '个文件含解析错误')
for f in files:
    print(' ', f)
cmds = {}
for e in d['errors']:
    line = e['line']
    cmd = line.split()[0].split('\t')[0] if line.split() else ''
    cmds[cmd] = cmds.get(cmd, 0) + 1
print('\n未知命令 Top:')
for k, v in sorted(cmds.items(), key=lambda x: -x[1])[:40]:
    print(f'  {v:4d}  {k}')
