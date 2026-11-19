# 逐帧 diff（旧夹具 vs 重生成夹具）：对每个 case 比 immediate_frames/notices/pumped/added_items + wabil(后)
# + wabil_before(元数据)，逐字段列出差异并分类：
#  - consumed: 该例 max_hp/max_mp 被消费(Shape1 clamp/Shape3 percent) → 帧面变化属"应同步"
#  - 52tail/state: 仅 wabil(后) 的 MaxHP/MaxWeight/DC 等元数据变(如发 SM52 则其尾带真实 WAbil) → "更真实非回归"
#  - metadata: 仅 wabil_before(输入记录) 变 → "仅夹具元数据变，帧面不变"
#  - UNEXPLAINED: 帧面变化但非消费例 → 回归嫌疑，需人判
import json,sys
CONSUMING={'eat_ok_shape1_fixed','eat_ok_shape1_clamp','eat_ok_shape3_percent'}
FRAME_KEYS=['immediate_frames','notices','pumped','added_items']
def load(p): return {c['name']:c for c in json.load(open(p,encoding='utf-8'))['cases']}
def fldiff(a,b,path=''):
    out=[]
    if type(a)!=type(b): out.append((path,'TYPE',repr(a),repr(b))); return out
    if isinstance(a,dict):
        for k in a.keys()|b.keys(): out+=fldiff(a.get(k),b.get(k),path+'/'+k)
    elif isinstance(a,list):
        if len(a)!=len(b): out.append((path,'LEN',len(a),len(b)))
        for i in range(min(len(a),len(b))): out+=fldiff(a[i],b[i],f'{path}[{i}]')
    else:
        if a!=b: out.append((path,'VAL',repr(a),repr(b)))
    return out
old=load(sys.argv[1]); new=load(sys.argv[2])
assert set(old)==set(new), (set(old)^set(new))
summary={'no-change':[], 'consumed':[], '52tail/state':[], '52tail-uncompared':[], 'metadata':[], 'UNEXPLAINED':[]}
for n in old:
    c_old,c_new=old[n],new[n]
    fd=[]; 
    for k in FRAME_KEYS:
        fd+=fldiff(c_old['expect'].get(k),c_new['expect'].get(k),k)
    wd=fldiff(c_old['expect'].get('wabil'),c_new['expect'].get('wabil'),'wabil')
    bd=fldiff(c_old.get('wabil_before'),c_new.get('wabil_before'),'wabil_before')
    if not fd and not wd and not bd:
        summary['no-change'].append(n); continue
    cat = 'consumed' if n in CONSUMING else ('UNEXPLAINED' if fd else ('52tail/state' if wd else 'metadata'))
    summary[cat].append(n)
    print(f"### {n}  -> {cat}")
    for path,kind,x,y in fd: print(f"   FRAME {path} {kind} {x} -> {y}")
    for path,kind,x,y in wd[:12]: print(f"   WABIL {path} {kind} {x} -> {y}")
    for path,kind,x,y in bd[:12]: print(f"   META  {path} {kind} {x} -> {y}")
print("\n=== SUMMARY ===")
for k,v in summary.items(): print(f"{k}: {len(v)} {v}")
