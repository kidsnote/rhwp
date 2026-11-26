#!/usr/bin/env python3
"""저장 줄 없는 문단: [TAC 표][공백 4][TAC 표][문단 끝 공백 15] 공개 재현물 (HWP5, 한컴 저장본 레코드 편집).

  python3 -I samples/issue9423/gen_two_tac_trailing_spaces.py \\
      samples/issue8574/tac_trailing_space-2020.hwp \\
      samples/issue9423/two_tac_trailing_spaces.hwp 50975 15 30000

바탕: samples/issue8574/tac_trailing_space-2020.hwp (한/글 2020 저장, 1쪽).
첫 표 30000HU 폭 + 공백 4 는 한 줄에 들고, 둘째 표 42400HU 는 다음 줄 머리에서 줄 폭(42520HU)을
거의 채운다. 그 뒤 문단 끝 공백 15 칸이 줄 폭을 넘는다. 첫 표 높이 50975HU 는 「끝」을 쪽 끝 한 줄
안쪽에 둔다. 한/글 2020 Print 는 끝 공백을 둘째 표 아래 줄로 넘겨 「끝」이 2쪽에 온다.
BodyText/Section0 에서 다음만 바꾼다.
  1. 본문(최상위) 문단의 PARA_LINE_SEG(레벨 1)를 지우고 PARA_HEADER 줄 수를 0 으로 둔다(칸 안 줄은 둔다).
  2. 문단 1(host)의 표 컨트롤(CTRL_HEADER~칸 문단)을 한 번 더 넣어 TAC 표 둘로 만들고, 둘째 표의
     인스턴스 id 만 바꾼다. 첫 표의 높이·폭(CTRL_HEADER·칸 LIST_HEADER)을 인자로 둔다.
  3. host 텍스트를 [표][공백 4][표][공백 N][문단 끝] 으로 두고 PARA_HEADER 글자 수를 맞춘다.
다른 레코드·스트림, OLE 섹터 배치는 원본 그대로다. 새 Section0 은 원래 스트림 길이에 맞춰 같은 자리에
쓴다(issue8463 생성기와 같은 sync flush + 빈 deflate 블록 채움). 필요: olefile.
"""
import shutil, struct, sys, zlib
import olefile

PARA_HEADER, PARA_TEXT, PARA_LINE_SEG, CTRL_HEADER, LIST_HEADER = 66, 67, 69, 71, 72
LINE_COUNT_OFFSET = 16
HOST = 1

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

def edit(plain, first_h, trail, first_w):
    recs = records(plain)
    top, header, out = -1, None, []
    i = 0
    while i < len(recs):
        tag, level, data = recs[i]
        if tag == PARA_HEADER and level == 0:
            top += 1; header = recs[i]
        if tag == PARA_LINE_SEG and level == 1:
            struct.pack_into('<H', header[2], LINE_COUNT_OFFSET, 0); i += 1; continue
        if top == HOST and level == 1 and tag == PARA_TEXT:
            assert data[0:2] == b'\x0b\x00' and data[16:18] == b' \x00' and data[-2:] == b'\r\x00'
            ctrl = bytes(data[0:16])
            new = ctrl + b' \x00' * 4 + ctrl + b' \x00' * trail + b'\r\x00'
            recs[i][2] = bytearray(new)
            nchars = struct.unpack_from('<I', header[2], 0)[0]
            struct.pack_into('<I', header[2], 0, (nchars & 0x8000_0000) | (len(new) // 2))
        if top == HOST and level == 1 and tag == CTRL_HEADER and data[0:4] == b' lbt':
            j = i + 1
            while j < len(recs) and recs[j][1] > 1: j += 1
            group = [[t, l, bytearray(d)] for t, l, d in recs[i:j]]
            second = [[t, l, bytearray(d)] for t, l, d in recs[i:j]]
            struct.pack_into('<II', group[0][2], 16, first_w, first_h)
            for r in group:
                if r[0] == LIST_HEADER:
                    struct.pack_into('<II', r[2], 16, first_w, first_h)
                    struct.pack_into('<I', r[2], 34, first_w)
            struct.pack_into('<I', second[0][2], 36, struct.unpack_from('<I', second[0][2], 36)[0] + 6)
            out += group + second; i = j; continue
        out.append(recs[i]); i += 1
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
first_h = int(sys.argv[3]) if len(sys.argv) > 3 else 8000
trail = int(sys.argv[4]) if len(sys.argv) > 4 else 15
first_w = int(sys.argv[5]) if len(sys.argv) > 5 else 30000
ole = olefile.OleFileIO(src)
assert struct.unpack_from('<I', ole.openstream('FileHeader').read(), 36)[0] & 1
raw = ole.openstream('BodyText/Section0').read(); ole.close()
new = fit(edit(zlib.decompress(raw, -15), first_h, trail, first_w), len(raw))
shutil.copyfile(src, out)
ole = olefile.OleFileIO(out, write_mode=True); ole.write_stream('BodyText/Section0', new); ole.close()
