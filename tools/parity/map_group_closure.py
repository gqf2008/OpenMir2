#!/usr/bin/env python3
"""地图组移动封闭性分析（P0-3）：从 Envir/MapInfo.txt 抽出口边，给弱连通分量与切分实验。

用法：
  python -I -B tools/parity/map_group_closure.py --mapinfo E:/MirServer/Mir200/Envir/MapInfo.txt
  python -I -B tools/parity/map_group_closure.py --selftest      # 离线合成夹具

口径：出口行 = `源图 x,y -> 目标图 x,y`；`[名字|文件名 描述] FLAGS` 是地图声明；
`NORECONNECT(x)` 等 Flag **不是出口**（断线/回城落点）。文件是 **GBK**。
"""
import argparse, collections, os, re, sys, tempfile

HDR = re.compile(r"^\[([^\]|\s]+)")
EXIT = re.compile(r"^(\S+)\s+(\d+)\s*,\s*(\d+)\s*->\s*(\S+)\s+(\d+)\s*,\s*(\d+)")

def parse(path):
    maps, edges = set(), set()
    with open(path, encoding="gbk", errors="replace") as f:
        for line in f:
            s = line.strip()
            if not s:
                continue
            m = EXIT.match(s)
            if m:
                edges.add((m.group(1), m.group(4)))
                maps.update((m.group(1), m.group(4)))
                continue
            m = HDR.match(s)
            if m:
                maps.add(m.group(1).strip())
    return maps, edges

def components(maps, edges):
    parent = {m: m for m in maps}
    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x
    for a, b in edges:
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[ra] = rb
    comps = collections.defaultdict(set)
    for m in maps:
        comps[find(m)].add(m)
    return sorted((len(v) for v in comps.values()), reverse=True)

def cross_by_prefix(maps, edges):
    def cls(n):
        return 1 if n[:1].upper() > "M" else 0
    return sorted({(a, b) for a, b in edges if cls(a) != cls(b)})

def report(path):
    maps, edges = parse(path)
    sizes = components(maps, edges)
    cross = cross_by_prefix(maps, edges)
    print("MAPS %d EDGES %d WCC %d SINGLETONS %d MAX %d" % (
        len(maps), len(edges), len(sizes), sum(1 for s in sizes if s == 1), sizes[0] if sizes else 0))
    print("PART_PREFIX cross_edges %d of %d" % (len(cross), len(edges)))
    for a, b in cross[:20]:
        print("CROSS %s -> %s" % (a, b))

def selftest():
    d = tempfile.mkdtemp(prefix="mapcl-")
    try:
        p = os.path.join(d, "MapInfo.txt")
        with open(p, "w", encoding="gbk") as f:
            f.write("A 甲\nA 1,1 -> B 2,2\nB 3,3 -> C 4,4\n[D 丁]\n[T9 九]\n")
        maps, edges = parse(p)
        assert maps == {"A", "B", "C", "D", "T9"}, maps
        assert edges == {("A", "B"), ("B", "C")}, edges
        sizes = components(maps, edges)
        assert sizes == [3, 1, 1], sizes
        cross = cross_by_prefix(maps, edges)
        assert cross == [], cross   # B->C 两端都 <= M；跨前缀边为 0
        print("MAP_GROUP_CLOSURE_SELFTEST_OK 4/4")
        return 0
    finally:
        import shutil
        shutil.rmtree(d, ignore_errors=True)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mapinfo", default=r"E:/MirServer/Mir200/Envir/MapInfo.txt")
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        return selftest()
    report(a.mapinfo)
    return 0

if __name__ == "__main__":
    sys.exit(main())
