#!/usr/bin/env python3
"""한 행의 칸 선언 높이가 서로 다를 때의 행 높이 조사용 공개 재현물 (HWP5, 한컴 저장본 레코드 편집).

  python3 -I gen_row_height_sweep.py samples/issue8493/table_tail.hwp <출력.hwp>

바탕: samples/issue8493/table_tail.hwp (한/글 2022 12.0.0.4547 저장). 문단 0 뒤에 표 문단과 이름표 문단
「Tn」 을 번갈아 둔다. 표는 바탕 표(자리차지, 글자처럼 아님, 셀 단위 나눔) 속성 그대로이고 2행 2열,
칸 폭은 같고 칸마다 한 줄 글 「Tt Rr Cc」. 행 1 은 두 칸 모두 1800HU. 행 0 의 두 칸 선언 높이와 표 전체
높이(CTRL_HEADER)는 아래 VARIANTS 대로다. 모든 PARA_LINE_SEG 를 두지 않는다.
다른 스트림, OLE 섹터 배치는 원본 그대로다(같은 길이 덮어쓰기). 필요: olefile.
"""
import shutil, struct, sys, zlib
import olefile

# (행 0 왼쪽 칸, 행 0 오른쪽 칸, 표 전체 높이: 'min' = 행마다 작은 값의 합, 'max' = 큰 값의 합)
VARIANTS = [
    (3000, 1800, 'min'),
    (1800, 3000, 'min'),
    (4000, 1800, 'min'),
    (2400, 1800, 'min'),
    (3000, 3000, 'min'),
    (3000, 1800, 'max'),
]
PARA_HEADER, PARA_TEXT, PARA_CHAR_SHAPE, PARA_LINE_SEG, CTRL_HEADER, LIST_HEADER, TABLE = 66, 67, 68, 69, 71, 72, 77

def records(buf):
    pos, out = 0, []
    while pos + 4 <= len(buf):
        h = struct.unpack_from('<I', buf, pos)[0]
        tag, level, size, hdr = h & 0x3FF, (h >> 10) & 0x3FF, h >> 20, 4
        if size == 0xFFF:
            size = struct.unpack_from('<I', buf, pos + 4)[0]; hdr = 8
        out.append([tag, level, bytearray(buf[pos + hdr:pos + hdr + size])])
        pos += hdr + size
    assert pos == len(buf)
    return out

def serialize(recs):
    out = bytearray()
    for tag, level, data in recs:
        size = len(data)
        out += struct.pack('<I', tag | (level << 10) | (min(size, 0xFFF) << 20))
        if size >= 0xFFF: out += struct.pack('<I', size)
        out += data
    return bytes(out)

def groups(recs, level):
    out = []
    for r in recs:
        if r[0] == PARA_HEADER and r[1] == level: out.append([r])
        elif out: out[-1].append(r)
    return out

INST = [0x4000]
def make_para(level, ph_tpl, cs_tpl, text, last):
    body = (text.encode('utf-16-le') if text else b'') + b'\r\x00'
    h = bytearray(ph_tpl)
    struct.pack_into('<I', h, 0, (len(body) // 2) | (0x8000_0000 if last else 0))
    struct.pack_into('<I', h, 4, 0)
    struct.pack_into('<H', h, 16, 0)
    INST[0] += 1
    struct.pack_into('<I', h, 18, INST[0])
    out = [[PARA_HEADER, level, h]]
    if text: out.append([PARA_TEXT, level + 1, bytearray(body)])
    out.append([PARA_CHAR_SHAPE, level + 1, bytearray(cs_tpl)])
    return out

def build(plain):
    recs = records(plain)
    top = groups(recs, 0)
    first = [r for r in top[0] if not (r[0] == PARA_LINE_SEG and r[1] == 1)]
    struct.pack_into('<H', first[0][2], 16, 0)
    pre = top[1]
    body_ph, body_cs = pre[0][2], next(r[2] for r in pre if r[0] == PARA_CHAR_SHAPE)
    host = next(g for g in top if any(r[0] == CTRL_HEADER and r[2][0:4] == b' lbt' for r in g))
    h_ph = bytearray(host[0][2]); struct.pack_into('<H', h_ph, 16, 0)
    h_text = next(r for r in host if r[0] == PARA_TEXT)
    h_cs = next(r for r in host if r[0] == PARA_CHAR_SHAPE)
    ctrl_tpl = next(r for r in host if r[0] == CTRL_HEADER)[2]
    table = next(r for r in host if r[0] == TABLE)[2]
    lh_tpl = next(r for r in host if r[0] == LIST_HEADER)[2]
    cell_groups = groups([r for r in host if r[1] >= 2 and r[0] not in (TABLE, LIST_HEADER)], 2)
    c_ph = cell_groups[0][0][2]; c_cs = next(r[2] for r in cell_groups[0] if r[0] == PARA_CHAR_SHAPE)
    cw = struct.unpack_from('<I', ctrl_tpl, 16)[0] // 2
    out = list(first)
    for t, (a, b, total) in enumerate(VARIANTS):
        out += make_para(0, body_ph, body_cs, f'T{t}', False)
        rows = [(a, b), (1800, 1800)]
        ctrl = bytearray(ctrl_tpl)
        pick = min if total == 'min' else max
        struct.pack_into('<I', ctrl, 20, sum(pick(r) for r in rows))
        struct.pack_into('<I', ctrl, 36, struct.unpack_from('<I', ctrl, 36)[0] + t + 1)
        new_table = bytearray(bytes(table[:4]) + struct.pack('<HH', 2, 2) + bytes(table[8:18])
                              + struct.pack('<H', 2) * 2 + bytes(table[-4:]))
        out += [[PARA_HEADER, 0, bytearray(h_ph)], h_text, h_cs, [CTRL_HEADER, 1, ctrl], [TABLE, 2, new_table]]
        for ri, hs in enumerate(rows):
            for ci, hh in enumerate(hs):
                lh = bytearray(lh_tpl)
                struct.pack_into('<H', lh, 0, 1)
                struct.pack_into('<HHHH', lh, 8, ci, ri, 1, 1)
                struct.pack_into('<II', lh, 16, cw, hh)
                struct.pack_into('<I', lh, 34, cw)
                out.append([LIST_HEADER, 2, lh])
                out += make_para(2, c_ph, c_cs, f'T{t} R{ri} C{ci}', True)
    out += make_para(0, body_ph, body_cs, 'END', True)
    return serialize(out)

def empty_tail(k, final_fixed):
    bits = []
    for _ in range(k): bits += [0, 1, 0] + [0] * 7
    bits += [1, 1, 0] + [0] * 7 if final_fixed else [1, 0, 0]
    bits += [0] * (-len(bits) % 8)
    out = bytes(sum(b << i for i, b in enumerate(bits[j:j + 8])) for j in range(0, len(bits), 8))
    return out if final_fixed else out + b'\x00\x00\xff\xff'

def fit(plain, length):
    for level in range(9, 0, -1):
        c = zlib.compressobj(level, zlib.DEFLATED, -15)
        head = c.compress(plain) + c.flush(zlib.Z_SYNC_FLUSH)
        pad = length - len(head)
        for m in range(max(pad // 5, 0) + 1):
            for k in range(8):
                for final_fixed in (False, True):
                    data = head + b'\x00\x00\x00\xff\xff' * m + empty_tail(k, final_fixed)
                    if len(data) == length:
                        assert zlib.decompress(data, -15) == plain
                        return data
    raise SystemExit(f'원래 스트림 길이에 맞출 수 없다 (필요 {len(head)} / {length})')

src, out = sys.argv[1:3]
ole = olefile.OleFileIO(src)
assert struct.unpack_from('<I', ole.openstream('FileHeader').read(), 36)[0] & 1
raw = ole.openstream('BodyText/Section0').read(); ole.close()
new = fit(build(zlib.decompress(raw, -15)), len(raw))
shutil.copyfile(src, out)
ole = olefile.OleFileIO(out, write_mode=True); ole.write_stream('BodyText/Section0', new); ole.close()
