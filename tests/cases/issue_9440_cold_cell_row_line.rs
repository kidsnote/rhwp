//! 관계형: 저장 줄 없는 HWP5 표 칸에서 글자 없이 단 정의 컨트롤만 든 문단도 줄 하나를 차지한다.
//! 주석 작성: 2026-11-26 KST (새 파일).
//!
//! # 무엇이 깨져 있었나
//!
//! HWP5 를 불러올 때 저장 줄 없는 칸 문단 가운데 컨트롤 없는 빈 문단만 글자 크기로 줄을 합성했다.
//! 단 정의(`cold`) 컨트롤만 든 빈 문단은 줄 없이 남아, 칸 높이 계산이 그 문단을 400HU 로 셌다.
//! 칸 내용이 한 줄(1600HU)만큼 짧게 잡혀 선언 높이가 이기고, 행이 짧아져 쪽 끝 문단이 앞 쪽에 남았다.
//! 같은 내용을 원본 HWPX 로 열면 그 문단이 줄 하나로 잡혀 이 차이가 없다.
//!
//! # 기대값의 출처
//!
//! - 재현물 `samples/issue9440/cold-cell-row-page-end.hwp` 는 한/글 2022 저장본
//!   `samples/issue8493/table_tail.hwp` 를 레코드만 고친 것이다(생성기 같은 폴더). 문서 전체에
//!   저장 줄이 없고, 3행 표의 행 1 은 [단 정의만 든 빈 문단][LINE A][LINE B][빈 문단] 이다.
//! - 한/글 2020 Print(`pdf/issue9440/cold-cell-row-page-end-2020.pdf`)는 2쪽이고 「END」만
//!   2쪽이다. 「LINE A」 글자 위끝은 행 1 위끝에서 줄 하나(약 16pt) 아래다.
//! - 대조군 `ctl-no-cold-page-end.hwp` 는 행 1 첫 문단만 컨트롤 없는 빈 문단으로 바꾼 같은 문서다
//!   (한/글 2020 Print 도 2쪽).
//!
//! # 검사하는 것
//!
//! - 쪽수 2, 「END」만 2쪽.
//! - 행 1 칸 높이가 대조군의 행 1 칸 높이와 같다(단 정의 컨트롤은 높이를 바꾸지 않는다).
//! - 「LINE A」는 행 1 위끝에서 「LINE A」·「LINE B」 줄 간격 이상 아래에서 시작한다(첫 빈 문단의 줄).

#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

const COLD: &str = "samples/issue9440/cold-cell-row-page-end.hwp";
const CONTROL: &str = "samples/issue9440/ctl-no-cold-page-end.hwp";

fn load(sample: &str) -> DocumentCore {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(sample);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("재현물 {sample} 읽기: {e}"));
    DocumentCore::from_bytes(&bytes).expect("문서 로드")
}

#[derive(Default)]
struct Page {
    /// 표 칸 (행, 위끝, 높이)
    cells: Vec<(u16, f64, f64)>,
    /// 글줄 (글, 위끝, 아래끝)
    lines: Vec<(String, f64, f64)>,
}

fn walk(node: &RenderNode, page: &mut Page) {
    match &node.node_type {
        RenderNodeType::TableCell(cell) if cell.col == 0 => {
            page.cells.push((cell.row, node.bbox.y, node.bbox.height));
        }
        RenderNodeType::TextLine(_) => {
            let mut text = String::new();
            collect_text(node, &mut text);
            page.lines.push((
                text.trim().to_string(),
                node.bbox.y,
                node.bbox.y + node.bbox.height,
            ));
        }
        _ => {}
    }
    for child in &node.children {
        walk(child, page);
    }
}

fn collect_text(node: &RenderNode, out: &mut String) {
    if let RenderNodeType::TextRun(run) = &node.node_type {
        out.push_str(&run.text);
    }
    for child in &node.children {
        collect_text(child, out);
    }
}

fn page(doc: &DocumentCore, index: u32) -> Page {
    let tree = doc
        .build_page_render_tree(index)
        .unwrap_or_else(|e| panic!("{}쪽 렌더 트리: {e:?}", index + 1));
    let mut page = Page::default();
    walk(&tree.root, &mut page);
    page
}

fn row_cell(page: &Page, row: u16) -> (f64, f64) {
    page.cells
        .iter()
        .find(|(r, ..)| *r == row)
        .map(|(_, top, height)| (*top, *height))
        .unwrap_or_else(|| panic!("행 {row} 칸이 없다: {:?}", page.cells))
}

fn line_top(page: &Page, text: &str) -> f64 {
    page.lines
        .iter()
        .find(|(t, ..)| t == text)
        .map(|(_, top, _)| *top)
        .unwrap_or_else(|| panic!("「{text}」 줄이 없다: {:?}", page.lines))
}

#[test]
fn column_def_only_cell_paragraph_pushes_last_paragraph() {
    let doc = load(COLD);
    assert_eq!(
        doc.page_count(),
        2,
        "한/글 2020 Print 는 2쪽(「END」만 2쪽)"
    );
    let first = page(&doc, 0);
    let second = page(&doc, 1);
    assert!(
        first.lines.iter().all(|(t, ..)| t != "END"),
        "「END」가 1쪽에 남았다: {:?}",
        first.lines
    );
    assert!(
        second.lines.iter().any(|(t, ..)| t == "END"),
        "2쪽에 「END」가 없다: {:?}",
        second.lines
    );
}

#[test]
fn column_def_only_cell_paragraph_keeps_row_height_of_plain_empty_paragraph() {
    let cold = page(&load(COLD), 0);
    let control = page(&load(CONTROL), 0);
    let (_, cold_height) = row_cell(&cold, 1);
    let (_, control_height) = row_cell(&control, 1);
    assert!(
        (cold_height - control_height).abs() <= 0.5,
        "단 정의만 든 빈 문단 행 높이 {cold_height:.1} 가 빈 문단 행 높이 {control_height:.1} 와 다르다"
    );
}

#[test]
fn column_def_only_cell_paragraph_occupies_one_line_above_text() {
    let first = page(&load(COLD), 0);
    let (row_top, row_height) = row_cell(&first, 1);
    let a = line_top(&first, "LINE A");
    let b = line_top(&first, "LINE B");
    let pitch = b - a;
    assert!(pitch > 0.0, "「LINE B」가 「LINE A」 아래가 아니다");
    assert!(
        a - row_top + 0.5 >= pitch,
        "「LINE A」({a:.1})는 행 1 위끝({row_top:.1})에서 줄 하나({pitch:.1}) 이상 아래여야 한다"
    );
    let b_bottom = first
        .lines
        .iter()
        .find(|(t, ..)| t == "LINE B")
        .map(|(_, _, bottom)| *bottom)
        .unwrap();
    assert!(
        row_top + row_height - b_bottom + 0.5 >= pitch * 0.5,
        "「LINE B」 아래끝({b_bottom:.1})과 행 1 아래끝({:.1}) 사이에 마지막 빈 문단 자리가 없다",
        row_top + row_height
    );
}
