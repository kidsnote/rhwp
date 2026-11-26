//! 관계형: 저장 줄 없는 문단에 글자처럼 취급 표가 둘이면, 둘째 표 뒤 넘친 문단 끝 공백은
//! 표 아래 자기 줄로 넘어간다(줄 머리 표 하나뿐인 문단의 공백 걸기와 구분).
//! 주석 작성: 2026-11-26 KST (새 파일).
//!
//! # 무엇이 깨져 있었나
//!
//! 줄 머리 글자처럼 취급 표 하나와 문단 끝 공백만 있는 문단은 한/글이 넘친 공백을 표 줄 끝에
//! 건다(`issue_8574_tac_host_trailing_space_line`). 렌더 인라인 흐름은 이 걸기를 문단 끝
//! 공백 전체에 적용해, 표 앞 공백이나 두 번째 표가 있는 문단에서도 넘친 공백 줄을 없앴다.
//! 그 줄 높이(줄높이 + 줄간격)만큼 뒤 내용이 올라가 쪽 끝 문단이 앞 쪽에 남았다.
//!
//! # 기대값의 출처
//!
//! - 재현물 `samples/issue9423/two_tac_trailing_spaces.hwp` 는 한/글 2020 저장본
//!   `samples/issue8574/tac_trailing_space-2020.hwp` 를 레코드만 고친 것이다(생성기 같은 폴더).
//!   host 문단은 [표 30000HU][공백 4][표 42400HU][문단 끝 공백 15] 이고 본문 저장 줄이 없다.
//! - 한/글 2020 Print(`pdf/issue9423/two_tac_trailing_spaces-2020.pdf`)는 2쪽이다.
//!   1쪽에 「앞 문단」·두 표·「표 뒤 첫 줄」, 2쪽에 「끝」. 「표 뒤 첫 줄」 글자 위끝은
//!   732.4pt(976.5px)로, 둘째 표 아래에 공백 줄 하나를 둔 자리다.
//!
//! # 검사하는 것
//!
//! - 쪽수 2, 「끝」만 2쪽.
//! - host 의 넘친 끝 공백은 둘째 표 아래에 자기 글줄을 갖고, 「표 뒤 첫 줄」은 그 줄 아래다.
//! - 대조: 줄 머리 표 하나 + 공백 문단은 여전히 공백 줄이 없다(issue8574 재현물).

#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

const TWO_TAC: &str = "samples/issue9423/two_tac_trailing_spaces.hwp";
const ONE_TAC: &str = "samples/issue8574/tac_trailing_space_nols.hwp";
const HOST_PARA: usize = 1;
const NEXT_PARA: usize = 2;
const LAST_PARA: usize = 3;

fn load(sample: &str) -> DocumentCore {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(sample);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("재현물 {sample} 읽기: {e}"));
    DocumentCore::from_bytes(&bytes).expect("문서 로드")
}

/// 본문 직속(표 안 제외) 글줄의 (문단, 위끝, 아래끝).
fn body_lines(node: &RenderNode, in_table: bool, out: &mut Vec<(usize, f64, f64)>) {
    if let RenderNodeType::TextLine(line) = &node.node_type {
        if !in_table {
            if let Some(para) = line.para_index {
                out.push((para, node.bbox.y, node.bbox.y + node.bbox.height));
            }
        }
    }
    let inside = in_table || matches!(node.node_type, RenderNodeType::Table(_));
    for child in &node.children {
        body_lines(child, inside, out);
    }
}

/// host 문단의 본문 직속 표 (위끝, 아래끝) 목록.
fn host_tables(node: &RenderNode, out: &mut Vec<(f64, f64)>) {
    if let RenderNodeType::Table(table) = &node.node_type {
        if table.para_index == Some(HOST_PARA) {
            out.push((node.bbox.y, node.bbox.y + node.bbox.height));
        }
        return;
    }
    for child in &node.children {
        host_tables(child, out);
    }
}

struct Page {
    tables: Vec<(f64, f64)>,
    lines: Vec<(usize, f64, f64)>,
}

fn page(doc: &DocumentCore, index: u32) -> Page {
    let tree = doc
        .build_page_render_tree(index)
        .unwrap_or_else(|e| panic!("{}쪽 렌더 트리: {e:?}", index + 1));
    let mut tables = Vec::new();
    host_tables(&tree.root, &mut tables);
    let mut lines = Vec::new();
    body_lines(&tree.root, false, &mut lines);
    Page { tables, lines }
}

fn paras(page: &Page) -> Vec<usize> {
    let mut paras: Vec<_> = page.lines.iter().map(|(para, ..)| *para).collect();
    paras.dedup();
    paras
}

#[test]
fn overflowing_tail_after_second_tac_table_pushes_last_paragraph() {
    let doc = load(TWO_TAC);
    assert_eq!(doc.page_count(), 2, "한/글 2020 Print 는 2쪽(「끝」만 2쪽)");
    let first = page(&doc, 0);
    let second = page(&doc, 1);
    assert!(
        !paras(&first).contains(&LAST_PARA),
        "「끝」이 1쪽에 남았다: {:?}",
        first.lines
    );
    assert_eq!(
        paras(&second),
        vec![LAST_PARA],
        "2쪽에는 「끝」만 와야 한다: {:?}",
        second.lines
    );
}

#[test]
fn overflowing_tail_after_second_tac_table_keeps_own_line() {
    let doc = load(TWO_TAC);
    let first = page(&doc, 0);
    assert_eq!(first.tables.len(), 2, "host 표 둘: {:?}", first.tables);
    let second_bottom = first
        .tables
        .iter()
        .map(|(_, bottom)| *bottom)
        .fold(f64::MIN, f64::max);
    let tail_line = first
        .lines
        .iter()
        .find(|(para, top, _)| *para == HOST_PARA && *top + 0.5 >= second_bottom)
        .unwrap_or_else(|| {
            panic!(
                "둘째 표(아래끝 {second_bottom:.1}) 아래에 host 끝 공백 줄이 없다: {:?}",
                first.lines
            )
        });
    let next_top = first
        .lines
        .iter()
        .filter(|(para, ..)| *para == NEXT_PARA)
        .map(|(_, top, _)| *top)
        .fold(f64::MAX, f64::min);
    assert!(
        next_top + 0.5 >= tail_line.2,
        "「표 뒤 첫 줄」({next_top:.1})은 host 끝 공백 줄(아래끝 {:.1}) 아래여야 한다",
        tail_line.2
    );
}

#[test]
fn single_leading_tac_table_tail_still_hangs() {
    let doc = load(ONE_TAC);
    assert_eq!(doc.page_count(), 1, "{ONE_TAC}: 한/글 Print 는 1쪽");
    let first = page(&doc, 0);
    let (_, table_bottom) = *first.tables.first().expect("host 표");
    let below: Vec<_> = first
        .lines
        .iter()
        .filter(|(para, top, _)| *para == HOST_PARA && *top + 0.5 >= table_bottom)
        .collect();
    assert!(
        below.is_empty(),
        "줄 머리 표 하나 + 끝 공백 문단에 공백 줄이 생겼다: {below:?}"
    );
}
