//! 장식 표도 행 내부의 내용 컷과 물리 높이를 같은 셀 유닛 결과로 전달한다.

use crate::model::table::{Table, TablePageBreak};
use crate::renderer::{
    layout::LayoutEngine,
    pagination::OverlayRowFragment,
    style_resolver::ResolvedStyleSet,
    typeset::{FormattedTable, TypesetEngine, TypesetState},
};

pub(super) fn query(
    engine: &TypesetEngine,
    state: &TypesetState,
    table: &Table,
    formatted: &FormattedTable,
    styles: &ResolvedStyleSet,
    row: usize,
    available: f64,
) -> Option<OverlayRowFragment> {
    if !matches!(table.layout_page_break(), TablePageBreak::RowBreak)
        || table.cells.iter().any(|cell| {
            usize::from(cell.row) <= row
                && row < usize::from(cell.row) + usize::from(cell.row_span)
                && cell.row_span > 1
        })
        || available <= 0.0
    {
        return None;
    }
    let layout = LayoutEngine::new(engine.dpi);
    engine.prime_section_cell_context(&layout);
    layout.set_layout_profile(state.profile);
    layout.prime_column_layout_env(&state.layout);
    layout.set_render_normalization_overlay(std::sync::Arc::clone(&engine.render_normalization));
    layout.remember_stored_frame_rows(table, &formatted.row_heights);
    // 유닛 진행은 내용 높이만 세므로 칸 안 여백까지 더한 여는 조각이 예산을 넘을 수 있다.
    // 그때는 넘친 만큼 예산을 줄여 다시 진행한다(유닛 경계가 바뀔 때까지 몇 번). 조각을 통째로
    // 버리면 행 전체가 다음 쪽 대기열로 가고, 이 쪽에 들어갈 앞 줄들까지 다음 쪽으로 밀린다.
    let mut budget = available;
    let mut fragment = None;
    for _ in 0..4 {
        let cut = layout.advance_row_cut(table, row, &[], budget, styles);
        if cut.fully_consumed || cut.end_cut.iter().all(|end| *end == 0) {
            return None;
        }
        // 강제 진행으로 얻은 컷은 실제 패딩까지 예산 안에 들 때만 수용한다.
        let content_height = layout.row_cut_content_height(table, row, &[], &cut.end_cut, styles);
        let source_frame = layout
            .stored_inline_opening_row_frame(
                table,
                row,
                &cut.end_cut,
                &formatted.row_heights,
                styles,
            )
            .filter(|height| *height <= available);
        let opening_height = source_frame.unwrap_or(content_height);
        if opening_height <= 0.0 {
            return None;
        }
        if opening_height <= available {
            fragment = Some((cut, opening_height, source_frame.is_some()));
            break;
        }
        budget -= opening_height - available;
        if budget <= 0.0 {
            return None;
        }
    }
    let (cut, opening_height, owns_stored_frame) = fragment?;
    let remaining_height = layout.row_cut_content_height(table, row, &cut.end_cut, &[], styles);
    Some(OverlayRowFragment {
        end_cut: cut.end_cut,
        opening_height,
        remaining_height,
        owns_stored_frame,
    })
}

/// 쪽 끝을 넘는 행의 글이 남은 자리에 다 들어가는지 칸마다 직접 비교한다.
///
/// 그 행에서 끝나는 칸마다 「여백 포함 내용 높이 ≤ 칸 위끝에서 본문 아래끝까지」 이어야 한다.
/// 세로 병합 칸은 끝나는 행에서 판정하고, 그 칸이 걸친 앞 행 높이만큼 자리를 더한다. 이 행을
/// 지나 더 내려가는 세로 병합 칸이 있으면 판정하지 않는다. 저장 줄이 있는 칸은 저장 좌표가
/// 따로 정하므로 대상이 아니다.
pub(super) fn whole_row_fits(
    engine: &TypesetEngine,
    state: &TypesetState,
    table: &Table,
    formatted: &FormattedTable,
    styles: &ResolvedStyleSet,
    row: usize,
    available: f64,
) -> bool {
    if !matches!(table.layout_page_break(), TablePageBreak::RowBreak) || available <= 0.0 {
        return false;
    }
    let covering: Vec<_> = table
        .cells
        .iter()
        .filter(|cell| {
            let top = usize::from(cell.row);
            top <= row && row < top + usize::from(cell.row_span.max(1))
        })
        .collect();
    if covering.is_empty()
        || covering
            .iter()
            .any(|cell| usize::from(cell.row) + usize::from(cell.row_span.max(1)) - 1 != row)
        || !covering
            .iter()
            .flat_map(|cell| &cell.paragraphs)
            .all(crate::renderer::para_has_no_stored_line_segs)
    {
        return false;
    }
    let layout = LayoutEngine::new(engine.dpi);
    engine.prime_section_cell_context(&layout);
    layout.set_layout_profile(state.profile);
    layout.prime_column_layout_env(&state.layout);
    layout.set_render_normalization_overlay(std::sync::Arc::clone(&engine.render_normalization));
    layout.remember_stored_frame_rows(table, &formatted.row_heights);
    // 남은 자리는 쪽 나눔 측정 행 높이와 그리기 행 높이 가운데 좁은 쪽으로 잰다. 중첩 표를 든
    // 행은 두 값이 다를 수 있다(961419 행 2: 측정 143.9px, 그리기 171.2px). 넓은 쪽을 믿으면
    // 실제로는 들어가지 않는 행을 이 쪽에 남긴다.
    let Some(&row_top) = formatted.cumulative_heights.get(row) else {
        return false;
    };
    let room = available + row_top;
    let painted_rows = layout.resolve_row_heights(
        table,
        usize::from(table.col_count),
        usize::from(table.row_count),
        None,
        styles,
        false,
    );
    let spacing = formatted.cell_spacing;
    let painted_top = |r: usize| -> Option<f64> {
        let rows = painted_rows.get(..r)?;
        Some(rows.iter().sum::<f64>() + spacing * r as f64)
    };
    for cell in &covering {
        let top = usize::from(cell.row);
        let (Some(&formatted_top), Some(painted)) =
            (formatted.cumulative_heights.get(top), painted_top(top))
        else {
            return false;
        };
        let space = (room - formatted_top).min(room - painted);
        let (_, _, pad_top, pad_bottom) = layout.resolve_cell_padding(cell, table);
        let content = layout.cell_units_content_height(cell, table, styles) + pad_top + pad_bottom;
        if content > space + 0.5 {
            return false;
        }
    }
    true
}
