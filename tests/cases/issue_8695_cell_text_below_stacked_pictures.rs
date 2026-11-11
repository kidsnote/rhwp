//! [#8576 후속] 칸 안에 겹침 허용 안 함으로 쌓인 어울림 그림 뒤의 글은 그림 아래로 간다.
//!
//! 표 칸의 글자 없는 첫 문단에 Square 그림 3장(앞 두 장은 나란히, 셋째는 두 장과
//! 겹쳐 그 아래로 쌓임)이 있고, 뒤에 빈 문단 하나와 글 문단 둘이 있다. 셋째 그림은
//! 칸 폭을 거의 다 덮어 옆에 글자 하나 들어갈 자리도 없다. 웹한글기안기 인쇄본
//! (`pdf/issue8695/*-webhwp-print.pdf`)은 빈 줄까지 모두 쌓인 그림 아래로
//! 보내 첫 글줄이 셋째 그림 바닥에서 두 줄 아래에 온다. 세로 가운데 정렬 칸도 같은
//! 순서로 놓고 내용 전체를 가운데로 옮긴다. rhwp 는 글줄을 그림과 겹쳐 그렸다.
//!
//! 쌓기가 없는 칸도 같다: 겹침 허용 그림 3장(`…-overlapping-pictures`), 한 줄로 나란한
//! 그림 2장(`…-row-pictures`, 그림 사이 빈 폭이 글자 하나보다 좁음). 인쇄본은 두 칸
//! 모두 첫 글줄을 그림 바닥에서 두 줄 아래(호스트 빈 줄, 빈 문단 다음)에 놓는다.
//!
//! 재현물은 공개 샘플(`samples/test-image.hwpx`, `samples/issue3738/…`)을 조합한 합성본이다.
//! 기대값은 관계(글줄이 그림과 겹치지 않음, 그림 아래에 있음, 칸 안에 있음)로 적는다.
#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use rhwp::document_core::DocumentCore;
use rhwp::renderer::render_tree::{RenderNode, RenderNodeType};

const TOP: &str = "samples/issue8695/cell-text-below-stacked-pictures-top.hwpx";
const CENTER: &str = "samples/issue8695/cell-text-below-stacked-pictures-center.hwpx";
const OVERLAPPING: &str = "samples/issue8695/cell-text-below-overlapping-pictures.hwpx";
const ROW: &str = "samples/issue8695/cell-text-below-row-pictures.hwpx";
const TOL_PX: f64 = 0.5;

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn rect(n: &RenderNode) -> Rect {
    Rect {
        x: n.bbox.x,
        y: n.bbox.y,
        w: n.bbox.width,
        h: n.bbox.height,
    }
}

/// 첫 쪽 그림 칸의 (칸, 그림들, 글이 있는 글줄들)
fn cell_parts(rel: &str) -> (Rect, Vec<Rect>, Vec<(String, Rect)>) {
    fn collect(n: &RenderNode, pics: &mut Vec<Rect>, lines: &mut Vec<(String, Rect)>) {
        match &n.node_type {
            RenderNodeType::Image(_) => pics.push(rect(n)),
            RenderNodeType::TextLine(_) => {
                let text: String = n
                    .children
                    .iter()
                    .filter_map(|c| match &c.node_type {
                        RenderNodeType::TextRun(run) => Some(run.text.clone()),
                        _ => None,
                    })
                    .collect();
                if !text.trim().is_empty() {
                    lines.push((text, rect(n)));
                }
            }
            _ => {}
        }
        for c in &n.children {
            collect(c, pics, lines);
        }
    }
    fn walk(n: &RenderNode, out: &mut Vec<(Rect, Vec<Rect>, Vec<(String, Rect)>)>) {
        if matches!(n.node_type, RenderNodeType::TableCell(_)) {
            let (mut pics, mut lines) = (Vec::new(), Vec::new());
            collect(n, &mut pics, &mut lines);
            out.push((rect(n), pics, lines));
            return;
        }
        for c in &n.children {
            walk(c, out);
        }
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let core = DocumentCore::from_bytes(&std::fs::read(path).expect("재현물")).expect("문서 열기");
    let tree = core.build_page_render_tree(0).expect("1쪽 render tree");
    let mut cells = Vec::new();
    walk(&tree.root, &mut cells);
    cells
        .into_iter()
        .find(|(_, pics, _)| !pics.is_empty())
        .expect("그림을 담은 칸")
}

fn assert_text_below_pictures(rel: &str, pic_count: usize) {
    let (cell, pics, lines) = cell_parts(rel);
    assert_eq!(pics.len(), pic_count, "그림 {pic_count}장: {pics:?}");
    let texts: Vec<&str> = lines.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(texts, ["가나다라마바사", "아자차카타파하"], "글 문단 순서");
    let stack_bottom = pics.iter().map(|p| p.y + p.h).fold(f64::MIN, f64::max);
    for (text, line) in &lines {
        for pic in &pics {
            let overlap = line.x < pic.x + pic.w - TOL_PX
                && pic.x < line.x + line.w - TOL_PX
                && line.y < pic.y + pic.h - TOL_PX
                && pic.y < line.y + line.h - TOL_PX;
            assert!(!overlap, "{text} 글줄 {line:?} 이 그림 {pic:?} 과 겹친다");
        }
        assert!(
            line.y >= stack_bottom - TOL_PX,
            "{text} 글줄({:.1})이 쌓인 그림 바닥({stack_bottom:.1}) 아래에 있어야 한다",
            line.y
        );
        assert!(
            line.y + line.h <= cell.y + cell.h + TOL_PX,
            "{text} 글줄이 칸 안에 있어야 한다"
        );
    }
    // 앞의 빈 줄 두 개(그림 호스트 줄, 빈 문단)도 그림 아래로 가므로 첫 글줄은
    // 그림 바닥에서 두 줄 아래다(쌓인 칸 인쇄본: 셋째 그림 바닥 188.6pt, 첫 글줄 위 약 220pt).
    let pitch = lines[1].1.y - lines[0].1.y;
    assert!(pitch > 0.0);
    let gap = lines[0].1.y - stack_bottom;
    assert!(
        (gap / pitch - 2.0).abs() < 0.25,
        "첫 글줄은 그림 바닥에서 두 줄 아래: gap={gap:.1} pitch={pitch:.1}"
    );
}

#[test]
fn cell_text_goes_below_stacked_square_pictures() {
    assert_text_below_pictures(TOP, 3);
}

#[test]
fn cell_text_goes_below_stacked_square_pictures_in_centered_cell() {
    assert_text_below_pictures(CENTER, 3);
    // 가운데 정렬: 내용(그림 위 ~ 마지막 글줄 아래)의 위아래 여백이 비슷하다.
    let (cell, pics, lines) = cell_parts(CENTER);
    let top = pics.iter().map(|p| p.y).fold(f64::MAX, f64::min);
    let bottom = lines.last().map(|(_, l)| l.y + l.h).unwrap();
    let (above, below) = (top - cell.y, cell.y + cell.h - bottom);
    assert!(
        (above - below).abs() < 25.0,
        "가운데 정렬 칸의 위({above:.1})·아래({below:.1}) 여백이 비슷해야 한다"
    );
}

#[test]
fn cell_text_goes_below_overlapping_square_pictures() {
    assert_text_below_pictures(OVERLAPPING, 3);
}

#[test]
fn cell_text_goes_below_square_pictures_in_one_row() {
    assert_text_below_pictures(ROW, 2);
}

// 그림 한 장이 칸 폭을 다 덮고 뒤에 이름표 문단이 하나 있는 칸(주간 계획안의 사진 칸 형상).
// 선언 칸 높이(25.1mm)는 그림(25.75mm)보다도 낮다. 한컴은 호스트 빈 줄을 그림 옆 폭 0
// 자리에 두고 이름표를 그림 바로 아래에 놓으며, 행을 그림 + 이름표만큼 키운다
// (한컴 저장 줄: 다음 문단 vpos = 그림 높이). 행이 늘지 않으면 이름표가 칸 밖으로 잘린다.
const SINGLE_CAPTION: &str = "samples/issue8695/cell-caption-below-single-picture.hwpx";
// 그림이 칸 오른쪽에 있고 짧은 글이 그 왼쪽 빈 자리에 들어가는 칸. 글은 그림 옆에 남는다.
const BESIDE_RIGHT: &str = "samples/issue8695/cell-text-beside-right-picture.hwpx";

#[test]
fn caption_below_single_full_width_picture_grows_row() {
    let (cell, pics, lines) = cell_parts(SINGLE_CAPTION);
    assert_eq!(pics.len(), 1, "그림 1장: {pics:?}");
    let pic = pics[0];
    let [(text, line)] = lines.as_slice() else {
        panic!("이름표 한 줄: {lines:?}");
    };
    assert_eq!(text, "가나다라");
    assert!(
        line.y >= pic.y + pic.h - TOL_PX,
        "이름표({:.1})는 그림 바닥({:.1}) 아래에 있어야 한다",
        line.y,
        pic.y + pic.h
    );
    assert!(
        line.y - (pic.y + pic.h) < line.h * 0.5,
        "호스트 빈 줄은 그림 아래로 내려가지 않는다: 이름표 {:.1}, 그림 바닥 {:.1}",
        line.y,
        pic.y + pic.h
    );
    assert!(
        line.y + line.h <= cell.y + cell.h + TOL_PX,
        "행이 그림 + 이름표를 담도록 커져야 한다: 칸 {cell:?}, 이름표 {line:?}"
    );
    assert!(pic.y >= cell.y - TOL_PX, "그림은 칸 위에서 시작한다");
}

/// 첫 쪽 그림 칸의 글이 있는 글자 조각(TextRun)들. 글줄(TextLine) 상자는 칸 폭 전체라
/// 그림과의 가로 겹침은 글자 조각으로 본다.
fn text_runs(rel: &str) -> Vec<(String, Rect)> {
    fn collect(n: &RenderNode, out: &mut Vec<(String, Rect)>) {
        if let RenderNodeType::TextRun(run) = &n.node_type {
            if !run.text.trim().is_empty() {
                out.push((run.text.clone(), rect(n)));
            }
        }
        for c in &n.children {
            collect(c, out);
        }
    }
    fn walk(n: &RenderNode, out: &mut Vec<Vec<(String, Rect)>>, has_pic: &mut Vec<bool>) {
        if matches!(n.node_type, RenderNodeType::TableCell(_)) {
            let mut runs = Vec::new();
            collect(n, &mut runs);
            fn any_pic(n: &RenderNode) -> bool {
                matches!(n.node_type, RenderNodeType::Image(_)) || n.children.iter().any(any_pic)
            }
            has_pic.push(any_pic(n));
            out.push(runs);
            return;
        }
        for c in &n.children {
            walk(c, out, has_pic);
        }
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let core = DocumentCore::from_bytes(&std::fs::read(path).expect("재현물")).expect("문서 열기");
    let tree = core.build_page_render_tree(0).expect("1쪽 render tree");
    let (mut cells, mut has_pic) = (Vec::new(), Vec::new());
    walk(&tree.root, &mut cells, &mut has_pic);
    cells
        .into_iter()
        .zip(has_pic)
        .find_map(|(runs, pic)| pic.then_some(runs))
        .expect("그림을 담은 칸")
}

#[test]
fn short_text_stays_beside_right_square_picture() {
    let (_, pics, _) = cell_parts(BESIDE_RIGHT);
    assert_eq!(pics.len(), 1, "그림 1장: {pics:?}");
    let pic = pics[0];
    let runs = text_runs(BESIDE_RIGHT);
    let texts: Vec<&str> = runs.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(texts, ["가나다", "라마바"], "글 문단 순서");
    let (_, first) = &runs[0];
    assert!(
        first.y < pic.y + pic.h - TOL_PX,
        "그림 왼쪽 빈 자리에 들어가는 글은 그림 아래로 내려가지 않는다: 글 {first:?}, 그림 {pic:?}"
    );
    for (text, run) in &runs {
        assert!(
            run.x + run.w <= pic.x + TOL_PX || run.y >= pic.y + pic.h - TOL_PX,
            "{text} 글자 {run:?} 가 그림 {pic:?} 과 겹친다"
        );
    }
}

// 칸 폭을 거의 다 채운 나란한 사진 3장(사진 사이 빈 폭은 글자 하나보다 좁음) 뒤에 빈 문단만
// 넷 있는 칸. 한컴은 뒤에 글이 없는 빈 문단들로 행을 키우지 않는다(실문서: 나란한 사진 3장 +
// 빈 문단 8개 칸이 사진 높이 그대로). 빈 문단을 사진 아래로 내리면 행이 커져 사진이 다음
// 쪽으로 밀린다.
const ROW_TRAILING_EMPTY: &str = "samples/issue8695/cell-row-pictures-trailing-empty-paras.hwpx";
// 나란한 사진 2장 사이 빈 자리(10mm)에 들어가는 화살표 문단과, 어느 빈 자리에도 들어가지
// 않는 긴 낱말 문단. 한컴은 낱말 단위로 빈 자리에 흘려 쓰므로 화살표 줄은 사진 띠에 남고,
// 긴 낱말 줄만 사진 아래로 간다.
const ARROWS_BETWEEN: &str = "samples/issue8695/cell-arrows-between-row-pictures.hwpx";

#[test]
fn trailing_empty_paragraphs_do_not_grow_row_of_side_by_side_pictures() {
    let (cell, pics, _) = cell_parts(ROW_TRAILING_EMPTY);
    assert_eq!(pics.len(), 3, "사진 3장: {pics:?}");
    let pic_bottom = pics.iter().map(|p| p.y + p.h).fold(f64::MIN, f64::max);
    let pic_top = pics.iter().map(|p| p.y).fold(f64::MAX, f64::min);
    for pic in &pics {
        assert!(
            (pic.y - pic_top).abs() < TOL_PX,
            "나란한 사진은 한 줄에 놓인다: {pics:?}"
        );
    }
    // 선언 높이 32mm(약 121px) 안에 사진(30mm)이 들어간다. 빈 문단 넷을 사진 아래로 내리면
    // 행이 사진 + 네 줄 이상으로 커진다.
    assert!(
        cell.h < (pic_bottom - pic_top) + 30.0,
        "뒤에 글이 없는 빈 문단은 행을 키우지 않는다: 칸 {cell:?}, 사진 {pics:?}"
    );
}

#[test]
fn word_fitting_between_pictures_stays_in_picture_band() {
    let (_, pics, _) = cell_parts(ARROWS_BETWEEN);
    assert_eq!(pics.len(), 2, "사진 2장: {pics:?}");
    let pic_bottom = pics.iter().map(|p| p.y + p.h).fold(f64::MIN, f64::max);
    let runs = text_runs(ARROWS_BETWEEN);
    let texts: Vec<&str> = runs.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(texts, ["→", "가나다라마바사아자차"], "글 문단 순서");
    assert!(
        runs[0].1.y < pic_bottom - TOL_PX,
        "사진 사이 빈 자리에 들어가는 화살표 줄은 사진 아래로 내려가지 않는다: {:?}, 사진 바닥 {pic_bottom:.1}",
        runs[0].1
    );
    assert!(
        runs[1].1.y >= pic_bottom - TOL_PX,
        "어느 빈 자리에도 들어가지 않는 낱말 줄은 사진 아래로 간다: {:?}, 사진 바닥 {pic_bottom:.1}",
        runs[1].1
    );
}
