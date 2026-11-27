//! 관계형: 쪽 끝을 넘는 저장 줄 없는 글앞으로 표(문단 기준, 셀 단위 나눔)의 행 배치.
//! 주석 작성: 2026-11-27 KST (새 파일).
//!
//! # 무엇이 깨져 있었나
//!
//! 쪽 끝을 넘는 글앞으로 표는 넘친 행을 다음 쪽 대기열로 넘긴다. 대기열은 뒤따르는 흐름이 쪽을
//! 열 때만 그려져, 표 뒤 흐름이 먼저 끝나면 넘친 행이 어디에도 그려지지 않았다. 또 글이 남은
//! 자리에 다 들어가는 행도 선언 높이가 쪽 끝을 넘는다는 이유로 통째로 넘겨 같은 식으로 사라졌다.
//!
//! # 기대값의 출처
//!
//! 재현물 두 개는 한/글 2022 저장본 `samples/issue8493/table_tail.hwp` 를 레코드만 고친 것이다
//! (생성기 `samples/issue9440/gen_overlay_tail.py`). 저장 줄이 없고 표 뒤 흐름은 「END」뿐이다.
//! - `overlay-tail-rows-overflow.hwp`: 12행, 행 6 부터 쪽 끝을 넘는다. 한/글 2020 Print
//!   (`pdf/issue9440/overlay-tail-rows-overflow-2020.pdf`)는 2쪽이고 R6~R11 이 2쪽 위에 온다.
//! - `overlay-tail-cell-taller.hwp`: 7행, 마지막 행은 선언 높이 3000/1800HU 로 쪽 끝을 넘지만 한 줄
//!   글은 남은 자리에 든다. 한/글 2020 Print 는 1쪽이고 마지막 행이 1쪽에 있으며, 표 아래선이 본문
//!   아래끝(769.05pt)에 맞춰 깎인다.
//!
//! # 검사하는 것
//!
//! - rows-overflow: 2쪽, R6~R11 글줄은 2쪽에만, R0~R5 는 1쪽에만 있다.
//! - cell-taller: 1쪽, 마지막 행 글 「R6 C0」·「R6 C1」이 1쪽에 그려지고 표가 본문 아래끝을 넘지 않는다.

#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

const ROWS_OVERFLOW: &str = "samples/issue9440/overlay-tail-rows-overflow.hwp";
const CELL_TALLER: &str = "samples/issue9440/overlay-tail-cell-taller.hwp";

fn load(sample: &str) -> DocumentCore {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(sample);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("재현물 {sample} 읽기: {e}"));
    DocumentCore::from_bytes(&bytes).expect("문서 로드")
}

fn collect_text(node: &RenderNode, out: &mut String) {
    if let RenderNodeType::TextRun(run) = &node.node_type {
        out.push_str(&run.text);
    }
    for child in &node.children {
        collect_text(child, out);
    }
}

fn lines(node: &RenderNode, out: &mut Vec<String>) {
    if let RenderNodeType::TextLine(_) = &node.node_type {
        let mut text = String::new();
        collect_text(node, &mut text);
        out.push(text.trim().to_string());
        return;
    }
    for child in &node.children {
        lines(child, out);
    }
}

fn page_lines(doc: &DocumentCore, index: u32) -> Vec<String> {
    let tree = doc
        .build_page_render_tree(index)
        .unwrap_or_else(|e| panic!("{}쪽 렌더 트리: {e:?}", index + 1));
    let mut out = Vec::new();
    lines(&tree.root, &mut out);
    out
}

fn has_row(lines: &[String], row: usize) -> bool {
    let prefix = format!("R{row} ");
    lines.iter().any(|line| line.starts_with(&prefix))
}

#[test]
fn overflowing_overlay_rows_open_next_page_after_flow_ends() {
    let doc = load(ROWS_OVERFLOW);
    assert_eq!(doc.page_count(), 2, "한/글 2020 Print 는 2쪽");
    let first = page_lines(&doc, 0);
    let second = page_lines(&doc, 1);
    for row in 0..6 {
        assert!(has_row(&first, row), "R{row} 가 1쪽에 없다: {first:?}");
        assert!(
            !has_row(&second, row),
            "R{row} 가 2쪽에 다시 있다: {second:?}"
        );
    }
    for row in 6..12 {
        assert!(has_row(&second, row), "R{row} 가 2쪽에 없다: {second:?}");
        assert!(!has_row(&first, row), "R{row} 가 1쪽에 있다: {first:?}");
    }
}

fn table_bottoms(node: &RenderNode, out: &mut Vec<f64>) {
    if let RenderNodeType::Table(_) = &node.node_type {
        out.push(node.bbox.y + node.bbox.height);
        return;
    }
    for child in &node.children {
        table_bottoms(child, out);
    }
}

fn body_bottom(node: &RenderNode) -> Option<f64> {
    if let RenderNodeType::Body { .. } = &node.node_type {
        return Some(node.bbox.y + node.bbox.height);
    }
    node.children.iter().find_map(body_bottom)
}

#[test]
fn overlay_row_whose_text_fits_stays_and_is_clipped_to_body() {
    let doc = load(CELL_TALLER);
    assert_eq!(doc.page_count(), 1, "한/글 2020 Print 는 1쪽");
    let first = page_lines(&doc, 0);
    for cell in ["R6 C0", "R6 C1"] {
        assert!(
            first.iter().any(|line| line == cell),
            "마지막 행 글 「{cell}」이 1쪽에 없다: {first:?}"
        );
    }
    let tree = doc.build_page_render_tree(0).expect("1쪽 렌더 트리");
    let mut bottoms = Vec::new();
    table_bottoms(&tree.root, &mut bottoms);
    let table_bottom = bottoms.iter().copied().fold(f64::MIN, f64::max);
    let body_bottom = body_bottom(&tree.root).expect("본문 영역");
    assert!(
        table_bottom <= body_bottom + 0.5,
        "표 아래끝 {table_bottom:.1} 이 본문 아래끝 {body_bottom:.1} 을 넘는다"
    );
}
