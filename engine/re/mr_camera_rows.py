"""Turn a mr_menu.py camera log (camera.json) into per-phase camera numbers (LIVE values in GAME.md): python mr_camera_rows.py camera.json"""
import json,struct,math,sys,collections
d=json.load(open(sys.argv[1])); ev=d["events"]
def fl(h): return struct.unpack("<%df"%(len(h)//8), bytes.fromhex(h))
fight=None; k=0; rows=[]
for e in ev:
    if e[3]=="fighter": fight=e[4]
    if e[3]=="pass" and fight:
        f=fl(e[4][1]); g=lambda a,n=3:f[(a-0x1405a5c00)//4:(a-0x1405a5c00)//4+n]
        r0,r1,r2=g(0x1405a5ca0),g(0x1405a5cb0),g(0x1405a5cc0); t=g(0x1405a5cd0)
        # view matrix, row-vector convention: columns of the 3x3 are right, up, forward
        right=(r0[0],r1[0],r2[0]); up=(r0[1],r1[1],r2[1]); fwd=(r0[2],r1[2],r2[2])
        eye=tuple(-(t[0]*right[i]+t[1]*up[i]+t[2]*fwd[i]) for i in range(3))
        proj=(g(0x1405a5c60,1)[0],g(0x1405a5c70,2)[1])
        view=k%2; k+=1
        F=fight[view]
        dx,dy,dz=(eye[0]-F[0],eye[1]-F[1],eye[2]-F[2])
        hd=math.hypot(dx,dz)
        # fighter in camera space
        rel=(F[0]-eye[0],F[1]-eye[1],F[2]-eye[2])
        along=sum(rel[i]*fwd[i] for i in range(3)); lat=sum(rel[i]*right[i] for i in range(3)); vert=sum(rel[i]*up[i] for i in range(3))
        pitch=math.degrees(math.asin(max(-1,min(1,fwd[1]))))
        rows.append(dict(tick=e[0],phase=e[2],view=view,eye=eye,fwd=fwd,proj=proj,F=F,hd=hd,h=dy,along=along,lat=lat,vert=vert,pitch=pitch))
json.dump(rows,open(sys.argv[1].replace(".json","_rows.json"),"w"))
by=collections.defaultdict(list)
for r in rows: by[(r["phase"],r["view"])].append(r)
for (ph,v),L in by.items():
    if v!=0: continue
    hd=[x["hd"] for x in L]; h=[x["h"] for x in L]; al=[x["along"] for x in L]; la=[x["lat"] for x in L]; vt=[x["vert"] for x in L]; pi=[x["pitch"] for x in L]
    print("%-10s n=%3d horiz-dist %.1f..%.1f  eye-above-F %.2f..%.2f  along %.1f..%.1f lat %.1f..%.1f vert %.1f..%.1f pitch %.1f..%.1f"%(ph,len(L),min(hd),max(hd),min(h),max(h),min(al),max(al),min(la),max(la),min(vt),max(vt),min(pi),max(pi)))
print("proj",set((round(r["proj"][0],4),round(r["proj"][1],4)) for r in rows))
