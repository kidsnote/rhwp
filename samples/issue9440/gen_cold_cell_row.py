#!/usr/bin/env python3
"""저장 줄 없는 칸의 「단 정의 컨트롤만 든 빈 문단」 행 높이 공개 재현물 (HWP5, 한컴 저장본 레코드 편집).

  python3 -I gen_cold_cell_row.py samples/issue8493/table_tail.hwp <출력.hwp> \\
      [--pre N] [--declared HU] [--head HU] [--tail N] [--no-cold]

  cold-cell-row-page-end.hwp : --pre 36 --declared 4900 --head 2400
  ctl-no-cold-page-end.hwp   : --pre 36 --declared 4900 --head 2400 --no-cold (대조군)

바탕: samples/issue8493/table_tail.hwp (한/글 2022 12.0.0.4547 저장, 1단 31행 표, 문단 모양 0 = 줄간격
160%, 글자 모양 1 = 10pt). BodyText/Section0 을 다음 순서로 다시 짠다. 레코드 바이트는 모두 바탕의
같은 종류 레코드를 복사해 글자 수·컨트롤 표시·줄 수·크기 칸만 고친다.
  문단 0  바탕 문단 0 그대로(구역 정의·단 정의).
  PRE     바탕의 한 줄 문단 「PRE nn」 을 N 개.
  표      바탕 표(글자처럼 취급 아님, 쪽 경계 셀 단위 나눔)를 3행 1열로 줄인다.
            행 0 「HEAD」(--head, 기본 1800HU), 행 1 = [단 정의 컨트롤만 든 빈 문단][「LINE A」][「LINE B」][빈 문단]
            (선언 높이 --declared), 행 2 「FOOT」(1800HU).
            단 정의 컨트롤(CTRL_HEADER 'cold')과 그 글자 자리는 바탕 문단 0 의 것을 그대로 복사한다.
  TAIL    「TAIL nn」 한 줄 문단 N 개, 마지막에 「END」.
  모든 PARA_LINE_SEG 를 두지 않는다(PARA_HEADER 줄 수 0, 저장 줄 없는 문서).
  --no-cold 는 행 1 첫 문단을 컨트롤 없는 빈 문단으로 둔 대조군이다.
다른 스트림, OLE 섹터 배치는 원본 그대로다. 새 Section0 은 원래 스트림 길이에 맞춰 같은 자리에 쓴다
(issue8463·issue9423 생성기와 같은 sync flush + 빈 deflate 블록 채움). 필요: olefile.
"""
import argparse, shutil, struct, zlib
import olefile

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
    """level 의 PARA_HEADER 로 시작하는 문단 묶음."""
    out = []
    for r in recs:
        if r[0] == PARA_HEADER and r[1] == level: out.append([r])
        elif out: out[-1].append(r)
    return out

class Inst:
    n = 0x2000
    @classmethod
    def next(cls):
        cls.n += 1; return cls.n

def make_para(level, ph_tpl, cs_tpl, text, last, cold=None):
    """text: str 또는 None(빈 문단). cold: (글자 자리 16바이트, CTRL_HEADER 바이트)."""
    body = b''
    mask = 0
    ctrl = []
    if cold:
        body += cold[0]; mask |= 1 << 2
        ctrl = [[CTRL_HEADER, level + 1, bytearray(cold[1])]]
    if text: body += text.encode('utf-16-le')
    body += b'\r\x00'
    h = bytearray(ph_tpl)
    struct.pack_into('<I', h, 0, (len(body) // 2) | (0x8000_0000 if last else 0))
    struct.pack_into('<I', h, 4, mask)
    struct.pack_into('<H', h, 16, 0)
    struct.pack_into('<I', h, 18, Inst.next())
    out = [[PARA_HEADER, level, h]]
    if len(body) > 2 or cold:
        out.append([PARA_TEXT, level + 1, bytearray(body)])
    out.append([PARA_CHAR_SHAPE, level + 1, bytearray(cs_tpl)])
    return out + ctrl

def build(plain, a):
    recs = records(plain)
    top = groups(recs, 0)
    first = [r for r in top[0] if not (r[0] == PARA_LINE_SEG and r[1] == 1)]
    struct.pack_into('<H', first[0][2], 16, 0)
    cold_chars = cold_ctrl = None
    for tag, level, data in top[0]:
        if tag == PARA_TEXT and level == 1:
            k = bytes(data).find(b'\x02\x00dloc')
            cold_chars = bytes(data[k:k + 16])
        if tag == CTRL_HEADER and level == 1 and data[0:4] == b'dloc':
            cold_ctrl = bytes(data)
    assert cold_chars and cold_ctrl
    pre = top[1]
    body_ph, body_cs = pre[0][2], next(r[2] for r in pre if r[0] == PARA_CHAR_SHAPE)
    host = next(g for g in top if any(r[0] == CTRL_HEADER and r[2][0:4] == b' lbt' for r in g))
    # host 문단: PARA_HEADER, PARA_TEXT, PARA_CHAR_SHAPE, (LINE_SEG), CTRL_HEADER, TABLE, LIST_HEADER...
    h_ph = bytearray(host[0][2]); struct.pack_into('<H', h_ph, 16, 0)
    h_text = next(r for r in host if r[0] == PARA_TEXT)
    h_cs = next(r for r in host if r[0] == PARA_CHAR_SHAPE)
    ctrl = bytearray(next(r for r in host if r[0] == CTRL_HEADER)[2])
    table = bytearray(next(r for r in host if r[0] == TABLE)[2])
    lh_tpl = next(r for r in host if r[0] == LIST_HEADER)[2]
    cell_groups = groups([r for r in host if r[1] >= 2 and r[0] != TABLE and r[0] != LIST_HEADER], 2)
    c_ph = cell_groups[0][0][2]; c_cs = next(r[2] for r in cell_groups[0] if r[0] == PARA_CHAR_SHAPE)

    rows = [
        (a.head, [('HEAD', None)]),
        (a.declared, ([] if not a.cold else [(None, (cold_chars, cold_ctrl))]) +
                     ([] if a.cold else [(None, None)]) +
                     [('LINE A', None), ('LINE B', None), (None, None)]),
        (1800, [('FOOT', None)]),
    ]
    # TABLE: attr4 rows2 cols2 spacing2 pad8 rowsizes2*R bf2 zones2
    head = bytes(table[:4]) + struct.pack('<HH', len(rows), 1) + bytes(table[8:18])
    tail = bytes(table[-4:])
    new_table = bytearray(head + struct.pack('<H', 1) * len(rows) + tail)
    struct.pack_into('<I', ctrl, 20, sum(h for h, _ in rows))
    out = [r for r in first]
    for n in range(a.pre):
        out += make_para(0, body_ph, body_cs, f'PRE {n + 1:02d}', False)
    out += [[PARA_HEADER, 0, h_ph], h_text, h_cs, [CTRL_HEADER, 1, ctrl], [TABLE, 2, new_table]]
    for ri, (hh, paras) in enumerate(rows):
        lh = bytearray(lh_tpl)
        struct.pack_into('<H', lh, 0, len(paras))
        struct.pack_into('<HH', lh, 8, 0, ri)
        struct.pack_into('<I', lh, 20, hh)
        out.append([LIST_HEADER, 2, lh])
        for pi, (text, cold) in enumerate(paras):
            out += make_para(2, c_ph, c_cs, text, pi == len(paras) - 1, cold)
    for n in range(a.tail):
        out += make_para(0, body_ph, body_cs, f'TAIL {n + 1:02d}', False)
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

ap = argparse.ArgumentParser()
ap.add_argument('src'); ap.add_argument('out')
ap.add_argument('--pre', type=int, default=2)
ap.add_argument('--declared', type=int, default=5400)
ap.add_argument('--tail', type=int, default=0)
ap.add_argument('--head', type=int, default=1800)
ap.add_argument('--no-cold', dest='cold', action='store_false')
a = ap.parse_args()
ole = olefile.OleFileIO(a.src)
assert struct.unpack_from('<I', ole.openstream('FileHeader').read(), 36)[0] & 1
raw = ole.openstream('BodyText/Section0').read(); ole.close()
new = fit(build(zlib.decompress(raw, -15), a), len(raw))
shutil.copyfile(a.src, a.out)
ole = olefile.OleFileIO(a.out, write_mode=True); ole.write_stream('BodyText/Section0', new); ole.close()
