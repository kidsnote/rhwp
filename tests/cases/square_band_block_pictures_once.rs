//! 한 문단에 나란히 놓인 그림 세 장이 든 행이 병합 칸 묶음 안에서 쪽 끝에 걸릴 때.
//!
//! 저장 줄(LINE_SEG)이 없는 칸의 글자 없는 문단 하나에 어울림(Square) 사진 두 장과 글자처럼
//! 취급한 사진 한 장이 한 줄 띠에 나란히 놓여 있다. 칸 선언 높이는 그림보다 낮다. 행을 가르는
//! 장부(cell_units)는 이 세 장을 세로로 쌓아 세고, 그리기는 한 띠로 그린다. 온전한 행 판정만
//! 그린 높이로 바꾸면 병합 칸 묶음을 가르는 쪽이 쌓은 높이로 띠를 잘라, 세 장 가운데 한 장만 앞
//! 쪽에 남고 그 한 장이 다음 쪽에 한 번 더 그려졌다(나머지 두 장은 줄이 어긋난다).
//!
//! 실문서(저장 줄 없는 생성계 활동 기록, 한 칸에 사진 3행 x 3장)에서 셋째 행의 좌우 두 장이
//! 사라지고 가운데 한 장이 두 쪽에 그려졌다. 그림 띠는 가를 수 없는 한 덩어리다: 세 장은 같은
//! 쪽, 같은 높이에 한 번씩 그려져야 한다.
//!
//! 재현물 `samples/square_photo_rows/square-band-block.hwp` 는 공개 샘플
//! (`samples/test-image.hwpx`, `samples/issue3738/tac_sibling_shape_line_advance.hwpx`)만 조합한
//! 합성본이다(저장 줄 없음, 원본 HWP5 경로). 3칸 표: 사진 행 6개, 칸 0 을 두 행 합친 묶음
//! (사진 행과 그림 세 장 띠 행), 꼬리 사진 행 1개. 띠 행은 그린 높이로는 1쪽에 남은 자리에 들고 쌓은
//! 높이로는 넘는다. 절대 좌표는 핀하지 않는다.
#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use rhwp::document_core::DocumentCore;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

const FIXTURE: &str = "samples/square_photo_rows/square-band-block.hwp";
/// 그림 세 장 띠가 든 행.
const BAND_ROW: u16 = 7;
/// 표 전체 그림 수: 사진 행 6 + 묶음 첫 행 1 + 띠 3 + 꼬리 행 1.
const PICTURES: usize = 11;

/// 쪽의 (행, 그림 위, 그림 왼쪽) 목록.
fn page_pictures(core: &DocumentCore, page: u32) -> Vec<(u16, f64, f64)> {
    fn walk(n: &RenderNode, row: Option<u16>, out: &mut Vec<(u16, f64, f64)>) {
        let row = match &n.node_type {
            RenderNodeType::TableCell(cell) => Some(cell.row),
            RenderNodeType::Image(_) => {
                if let Some(row) = row {
                    out.push((row, n.bbox.y, n.bbox.x));
                }
                row
            }
            _ => row,
        };
        for c in &n.children {
            walk(c, row, out);
        }
    }
    let tree = core.build_page_render_tree(page).expect("render tree");
    let mut out = Vec::new();
    walk(&tree.root, None, &mut out);
    out
}

fn load() -> DocumentCore {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    DocumentCore::from_bytes(&bytes).expect("문서 로드")
}

/// 표의 그림은 쪽을 넘어도 한 번씩만 그려진다.
#[test]
fn every_picture_is_painted_exactly_once() {
    let core = load();
    let total: usize = (0..core.page_count())
        .map(|page| page_pictures(&core, page).len())
        .sum();
    assert_eq!(
        total, PICTURES,
        "그림 {PICTURES}장이 빠지거나 겹치지 않고 한 번씩 그려져야 한다"
    );
}

/// 한 줄 띠의 그림 세 장은 같은 쪽, 같은 높이에 나란히 그려진다.
#[test]
fn band_pictures_stay_side_by_side_on_one_page() {
    let core = load();
    let bands: Vec<(u32, Vec<(u16, f64, f64)>)> = (0..core.page_count())
        .map(|page| {
            let band = page_pictures(&core, page)
                .into_iter()
                .filter(|(row, _, _)| *row == BAND_ROW)
                .collect::<Vec<_>>();
            (page, band)
        })
        .filter(|(_, band)| !band.is_empty())
        .collect();
    assert_eq!(
        bands.len(),
        1,
        "띠의 그림은 한 쪽에만 그려져야 한다: {bands:?}"
    );
    let (_, band) = &bands[0];
    assert_eq!(band.len(), 3, "띠의 그림 세 장: {band:?}");
    let top = band[0].1;
    assert!(
        band.iter().all(|(_, y, _)| (y - top).abs() <= 0.5),
        "세 장의 위가 같아야 한다: {band:?}"
    );
    let mut lefts: Vec<f64> = band.iter().map(|(_, _, x)| *x).collect();
    lefts.sort_by(f64::total_cmp);
    assert!(
        lefts.windows(2).all(|pair| pair[1] - pair[0] > 1.0),
        "세 장은 가로로 나란해야 한다: {band:?}"
    );
}
