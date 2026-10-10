# Independent reference for SBTS_vs / SBTS_ps, written instruction by instruction
# from tools/disasm/disasm.exe output (docs/STAGE-LIGHT.md), sampling the REAL
# spec77.dds with clamp + bilinear exactly like sampler s3.
import struct, math, sys
LUT = open(r'../../../game_files/Assets/Data/spec77.dds','rb').read()[128:]

def texel(x, y):
    x = min(max(x, 0), 255); y = min(max(y, 0), 255)
    o = (y * 256 + x) * 4
    b, g, r, a = LUT[o], LUT[o+1], LUT[o+2], LUT[o+3]
    return (r / 255, g / 255, b / 255, a / 255)

def sample(u, v):
    # clamp address, bilinear, texel centres at (i + 0.5) / 256
    fx = min(max(u * 256 - 0.5, 0), 255); fy = min(max(v * 256 - 0.5, 0), 255)
    x0, y0 = int(math.floor(fx)), int(math.floor(fy)); tx, ty = fx - x0, fy - y0
    out = []
    for c in range(4):
        a = texel(x0, y0)[c]; b = texel(x0+1, y0)[c]; cc = texel(x0, y0+1)[c]; d = texel(x0+1, y0+1)[c]
        out.append((a*(1-tx)+b*tx)*(1-ty) + (cc*(1-tx)+d*tx)*ty)
    return out

def dot(a, b): return sum(x*y for x, y in zip(a, b))
def sat(x): return min(max(x, 0.0), 1.0)
def add(a, b): return [x+y for x, y in zip(a, b)]
def mul(a, s): return [x*s for x in a]
def norm(a):
    l = math.sqrt(dot(a, a)); return [x / l for x in a]

def vs(T, B, N, pos, eye, DL, S0, P0pos, P0w, fres_k, vtx_a):
    """returns (o3 hemi xyzw, o4 point xyz, o5 dir, o6 half) -- raw outputs before the +0.5 encode"""
    # dp3 r0.x,-S0,T ; r0.y,-S0,B ; r0.z,+S0,N ; o3 = r0 * 0.5 + 0.5
    r0 = [-dot(S0, T), -dot(S0, B), dot(S0, N)]
    o3 = [x*0.5+0.5 for x in r0]
    # r0 = eye - pos ; len ; r1 = r0/len ; r0 = r0/len - DL ; r0.w = max(r1.N,0)
    e = [eye[i]-pos[i] for i in range(3)]; ln = math.sqrt(dot(e, e)); r1 = [x/ln for x in e]
    r0v = [r1[i] - DL[i] for i in range(3)]
    nv = max(dot(r1, N), 0.0)
    o3w = vtx_a * fres_k - vtx_a * nv
    # point light
    d = [pos[i]-P0pos[i] for i in range(3)]; d2 = dot(d, d); dl = math.sqrt(d2)
    u = [x/dl for x in d]
    r2 = [-dot(u, T), -dot(u, B), dot(u, N)]
    p = [-x*P0w for x in r2]; p = [x*(1.0/d2) for x in p]
    o4 = [x*0.5+0.5 for x in p]
    # directional: o5 = (-DL.T, -DL.B, -DL.N)
    o5 = [-dot(DL, T), -dot(DL, B), -dot(DL, N)]
    # half: normalize(r0v) in the frame
    h = norm(r0v)
    o6 = [dot(h, T), dot(h, B), dot(h, N)]
    return o3 + [o3w], o4, o5, o6

def ps(tex, nm, v3, v4, v5, v6, c2, c3, c5, c6, c7, ao_in):
    """tex = t0 rgba, nm = t1 rgba, v3/v4 = interpolants (xyzw), returns (rgb, alpha)"""
    n = [2*(x-0.5) for x in nm[:3]]
    r2x = dot(n, v5); r2y = dot(n, v6)
    lut = sample(r2x, r2y)                       # sample r2.xyzw, r2.xyxx, t3
    r3x = sat(lut[2]) * 0.8 + 0.00806452         # mad r3.x, sat(r2.z), c0.z, c0.w
    alpha = 2*r3x*((tex[3]-0.5)*2-0.5)           # dp2 r0.w, r3.xxxx, r0.wwww
    cl = lambda v: [min(max(x, -1.0), 1.0) for x in v]
    pt = cl(v4); hm = cl(v3)
    a_ts = [2*(x-0.5) for x in hm[:3]]
    lp = [2*(x-0.5) for x in pt[:3]]
    r4x = dot(a_ts, n)
    r1x = sat(dot(lp, n))                         # dp3_sat r1.x, r3.xyz, r1.xyz
    r2c = [sat(lut[i] * c3[i]) for i in range(3)] # mul_sat r2.xyz, r2.xyz, cb0[3]
    h = r4x * c7[3] + c7[3]                       # mad r1.y, r4.x, cb0[7].w, cb0[7].w
    hz = sat(h); h1 = 1 - h
    r3 = [h1 * c7[i] for i in range(3)]
    r3 = [sat(hz * c6[i] + r3[i]) for i in range(3)]
    spec = nm[3] * lut[3]                         # mul r1.y, r1.w, r2.w
    p1 = [sat(r1x * c5[i]) for i in range(3)]     # mul_sat r1.xzw, r1.xxxx, cb0[5]
    p2 = [sat(p1[i] * c5[i] + r2c[i]) for i in range(3)]   # mad_sat r1.xzw, r1.xxzw, cb0[5], r2
    spec = hm[3] * spec                           # mul r1.y, r4.w, r1.y
    amb = [tex[i] * r3[i] for i in range(3)]
    col = [sat(p2[i] * tex[i] + amb[i]) for i in range(3)]
    ao = pt[3]
    spc = [sat(spec * c2[i]) for i in range(3)]
    col = [max(ao * col[i], 0.0) + spc[i] for i in range(3)]
    return col, alpha

def run(name, T, B, N, pos, eye, DL, S0, P0pos, P0w, k, vtx_a, ao, tex, nm, dl_col, s0c, s1c, p0c, spec_rgb):
    o3, o4, o5, o6 = vs(T, B, N, pos, eye, DL, S0, P0pos, P0w, k, vtx_a)
    o4w = ao
    c2 = [spec_rgb[i]*2*dl_col[i] for i in range(3)]
    col, a = ps(tex, nm, o3, o4 + [o4w], o5, o6, c2, dl_col, p0c + [0.5], s0c + [0.5], s1c + [0.5], ao)
    print(f"{name}: rgb ({col[0]:.6f}, {col[1]:.6f}, {col[2]:.6f}) alpha {a:.6f}")
    return col, a

X, Y, Z = [1, 0, 0], [0, 1, 0], [0, 0, 1]
zero = [0, 0, 0]
DL_COL = [0.8, 0.6, 0.4]; S0C = [0.2, 0.25, 0.3]; S1C = [0.1, 0.1, 0.05]
base = [0.5, 0.5, 0.5, 1.0]
flat = [0.5, 0.5, 1.0, 1.0]
def nmap(v):  # tangent-space normal -> texel
    return [v[0]*0.5+0.5, v[1]*0.5+0.5, v[2]*0.5+0.5, 1.0]
far = [0, 0, 1e6]
# A: facing the light, flat map, hemisphere axis along N, no spec
run('A facing', X, Y, Z, zero, [0, 0, 10], [0, 0, -1], [0, 0, 1], far, 0.0, 0.0, 1.0, 1.0, base, flat, DL_COL, S0C, S1C, [0, 0, 0], [0, 0, 0])
# B: normal map tilted 60 deg toward +X, light off -> hemisphere only
run('B hemi 60deg', X, Y, Z, zero, [0, 0, 10], [0, 0, -1], [0, 0, 1], far, 0.0, 0.0, 1.0, 1.0, base, nmap([0.866025, 0, 0.5]), [0, 0, 0], S0C, S1C, [0, 0, 0], [0, 0, 0])
# C: frame signs: light tilted to +X (to_light (0.6,0,0.8)); map tilted +X vs -X
DLc = [-0.6, 0, -0.8]
run('C toward', X, Y, Z, zero, [0, 0, 10], DLc, [0, 0, 1], far, 0.0, 0.0, 1.0, 1.0, base, nmap([0.7071, 0, 0.7071]), DL_COL, [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0])
run('C away', X, Y, Z, zero, [0, 0, 10], DLc, [0, 0, 1], far, 0.0, 0.0, 1.0, 1.0, base, nmap([-0.7071, 0, 0.7071]), DL_COL, [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0])
axc = [0.6, 0, 0.8]
run('C hemi same', X, Y, Z, zero, [0, 0, 10], [0, 0, -1], axc, far, 0.0, 0.0, 1.0, 1.0, base, nmap([0.7071, 0, 0.7071]), [0, 0, 0], [1, 1, 1], [0, 0, 0], [0, 0, 0], [0, 0, 0])
run('C hemi opposite', X, Y, Z, zero, [0, 0, 10], [0, 0, -1], axc, far, 0.0, 0.0, 1.0, 1.0, base, nmap([-0.7071, 0, 0.7071]), [0, 0, 0], [1, 1, 1], [0, 0, 0], [0, 0, 0], [0, 0, 0])
# D: point light straight above at d=10, scalar 50
P0 = [0.8, 0.6, 0.4]
run('D point d=10', X, Y, Z, zero, [0, 0, 10], [0, 0, -1], [0, 0, 1], [0, 0, 10], 50.0, 0.0, 1.0, 1.0, base, flat, [0, 0, 0], [0, 0, 0], [0, 0, 0], P0, [0, 0, 0])
run('D point d=20', X, Y, Z, zero, [0, 0, 10], [0, 0, -1], [0, 0, 1], [0, 0, 20], 50.0, 0.0, 1.0, 1.0, base, flat, [0, 0, 0], [0, 0, 0], [0, 0, 0], P0, [0, 0, 0])
# E: specular, k=2.0 (short 100), camera on the normal, light along the normal
spc = [1.0, 128/255, 0.0]
run('E spec', X, Y, Z, zero, [0, 0, 10], [0, 0, -1], [0, 0, 1], far, 0.0, 2.0, 1.0, 1.0, base, flat, DL_COL, [0, 0, 0], [0, 0, 0], [0, 0, 0], spc)
run('E spec w=0', X, Y, Z, zero, [0, 0, 10], [0, 0, -1], [0, 0, 1], far, 0.0, 2.0, 0.0, 1.0, base, flat, DL_COL, [0, 0, 0], [0, 0, 0], [0, 0, 0], spc)
# F: ao
run('F ao 0.5', X, Y, Z, zero, [0, 0, 10], [0, 0, -1], [0, 0, 1], far, 0.0, 0.0, 1.0, 0.5, base, flat, DL_COL, S0C, S1C, [0, 0, 0], [0, 0, 0])
# G: alpha cases
run('G facing away', X, Y, Z, zero, [0, 0, 10], [0, 0, -1], [0, 0, 1], far, 0.0, 0.0, 1.0, 1.0, base, nmap([0, 0, -1]), DL_COL, S0C, S1C, [0, 0, 0], [0, 0, 0])
run('G alpha 0.75', X, Y, Z, zero, [0, 0, 10], [0, 0, -1], [0, 0, 1], far, 0.0, 0.0, 1.0, 1.0, [0.5, 0.5, 0.5, 0.75], flat, DL_COL, S0C, S1C, [0, 0, 0], [0, 0, 0])
# H: st03-like set, light tilted, camera oblique, tilted map, spec, the whole thing
DLh = norm([0.546, -0.677, -0.4925]); axh = norm([0.0076, 0.9961, -0.0876])
run('H st03-like', norm([1, 0, 0.1]), Y, norm([-0.1, 0, 1]), [3, 4, 5], [40, 30, 60], DLh, axh, [0, 0, 0], 0.0, 1.5, 0.8, 0.9, [0.4, 0.5, 0.6, 1.0], nmap(norm([0.2, -0.3, 0.93])),
    [105/255, 120/255, 130/255], [125/255, 130/255, 135/255], [20/255, 27/255, 30/255], [0, 0, 0], [0.3, 0.3, 0.3])
