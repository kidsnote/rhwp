#!/usr/bin/env python3
"""쪽 끝을 넘는 글앞으로 표(문단 기준, 셀 단위 나눔) 공개 재현물 (HWP5, 한컴 저장본 레코드 편집).

  python3 -I gen_overlay_tail.py samples/issue8493/table_tail.hwp <출력.hwp> --pre N --rows R
      [--last-c0 HU] [--last-c1 HU] [--row HU] [--voff HU]

바탕: samples/issue8493/table_tail.hwp (한/글 2022 12.0.0.4547 저장, 문단 모양 0 = 160%, 글자 모양 1 = 10pt).
BodyText/Section0 을 다음 순서로 다시 짠다(레코드 바이트는 바탕의 같은 종류 레코드를 복사).
  문단 0  바탕 문단 0(구역·단 정의).
  PRE     한 줄 문단 「PRE nn」 N 개.
  표 문단 R행 2열 표. 글자처럼 취급 아님, 본문과의 배치 = 글 앞으로(표 속성 비트 21-23 = 3),
          세로 기준 문단·오프셋 --voff(기본 1988HU, host 줄 아래에서 시작해 host 와 한 블록으로 묶이지 않음), 셀 단위 나눔(바탕 그대로). 행 0..R-2 는 두 칸 모두 --row(기본 1800HU),
          마지막 행은 왼쪽 칸 --last-c0, 오른쪽 칸 --last-c1 선언 높이. 표 전체 높이(CTRL_HEADER)는
          행마다 두 칸 중 **작은** 선언 높이를 더한 값으로 둔다. 칸마다 한 줄 글 「Rr Cc」.
  END     「END」 한 문단(표 문단 뒤 흐름은 이것뿐).
  모든 PARA_LINE_SEG 를 두지 않는다(PARA_HEADER 줄 수 0, 저장 줄 없는 문서).
  overlay-tail-cell-taller.hwp   : --pre 33 --rows 7 --last-c0 3000 --last-c1 1800
  overlay-tail-rows-overflow.hwp : --pre 34 --rows 12
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
    out = []
    for r in recs:
        if r[0] == PARA_HEADER and r[1] == level: out.append([r])
        elif out: out[-1].append(r)
    return out

INST = [0x3000]
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

def build(plain, a):
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
    ctrl = bytearray(next(r for r in host if r[0] == CTRL_HEADER)[2])
    table = bytearray(next(r for r in host if r[0] == TABLE)[2])
    lh_tpl = next(r for r in host if r[0] == LIST_HEADER)[2]
    cell_groups = groups([r for r in host if r[1] >= 2 and r[0] not in (TABLE, LIST_HEADER)], 2)
    c_ph = cell_groups[0][0][2]; c_cs = next(r[2] for r in cell_groups[0] if r[0] == PARA_CHAR_SHAPE)

    total_w = struct.unpack_from('<I', ctrl, 16)[0]
    cw = total_w // 2
    rows = [(a.row, a.row)] * (a.rows - 1) + [(a.last_c0, a.last_c1)]
    attr = struct.unpack_from('<I', ctrl, 4)[0]
    attr = (attr & ~1 & ~(7 << 21)) | (3 << 21)         # 글자처럼 아님, 글 앞으로
    struct.pack_into('<I', ctrl, 4, attr)
    struct.pack_into('<I', ctrl, 8, a.voff)             # 세로 오프셋
    struct.pack_into('<I', ctrl, 20, sum(min(r) for r in rows))
    head = bytes(table[:4]) + struct.pack('<HH', len(rows), 2) + bytes(table[8:18])
    new_table = bytearray(head + struct.pack('<H', 2) * len(rows) + bytes(table[-4:]))
    out = list(first)
    for n in range(a.pre):
        out += make_para(0, body_ph, body_cs, f'PRE {n + 1:02d}', False)
    out += [[PARA_HEADER, 0, h_ph], h_text, h_cs, [CTRL_HEADER, 1, ctrl], [TABLE, 2, new_table]]
    for ri, hs in enumerate(rows):
        for ci, hh in enumerate(hs):
            lh = bytearray(lh_tpl)
            struct.pack_into('<H', lh, 0, 1)
            struct.pack_into('<HHHH', lh, 8, ci, ri, 1, 1)
            struct.pack_into('<II', lh, 16, cw, hh)
            struct.pack_into('<I', lh, 34, cw)
            out.append([LIST_HEADER, 2, lh])
            out += make_para(2, c_ph, c_cs, f'R{ri} C{ci}', True)
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
ap.add_argument('--pre', type=int, default=30)
ap.add_argument('--rows', type=int, default=8)
ap.add_argument('--row', type=int, default=1800)
ap.add_argument('--voff', type=int, default=1988)
ap.add_argument('--last-c0', type=int, default=1800)
ap.add_argument('--last-c1', type=int, default=1800)
a = ap.parse_args()
ole = olefile.OleFileIO(a.src)
assert struct.unpack_from('<I', ole.openstream('FileHeader').read(), 36)[0] & 1
raw = ole.openstream('BodyText/Section0').read(); ole.close()
new = fit(build(zlib.decompress(raw, -15), a), len(raw))
shutil.copyfile(a.src, a.out)
ole = olefile.OleFileIO(a.out, write_mode=True); ole.write_stream('BodyText/Section0', new); ole.close()
