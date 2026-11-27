//! 장식 표의 현재 쪽 컷과 다음 쪽 예약량 조회.
//! Shape 발행·표 포맷·대기열 반영·host 텍스트 배치는 수행하지 않는다.
mod row_fragment;

use super::super::FormattedTable;
use crate::model::{paragraph::Paragraph, table::Table};
use crate::renderer::hwpunit_to_px;
use crate::renderer::pagination::OverlayRowFragment;

pub(in crate::renderer::typeset) struct OverlayContinuation {
    pub first_unfit: usize,
    pub remaining_px: f64,
    pub reserve_px: f64,
    pub room: f64,
    pub row_fragment: Option<OverlayRowFragment>,
    /// 넘치는 행의 글이 남은 자리에 다 들어가 그 행을 이 쪽에 두고 쪽 끝 너머 빈 몫만 버렸다.
    /// 다음 쪽 대기열은 그 다음 행부터이고, 남은 행이 없으면 대기열을 만들지 않는다.
    pub kept_whole_row: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn continuation(
    para: &Paragraph,
    table: &Table,
    next_para: Option<&Paragraph>,
    ft: &FormattedTable,
    current_height: f64,
    dpi: f64,
    base_available_height: impl FnOnce() -> f64,
    whole_row_fits: impl FnOnce(usize, f64) -> bool,
    partial_row: impl FnOnce(usize, f64) -> Option<OverlayRowFragment>,
) -> Option<OverlayContinuation> {
    // [#7441] 잔여 행 컷은 흐름을 따라 쪽 하단에 닿는 문단 기준 표만 대상이다(#4568 원 범위).
    // 아래 앵커 식(흐름 높이 + 오프셋)도 문단 기준에서만 성립한다. 용지 기준 표는 흐름과
    // 무관한 절대 위치 개체라 한컴은 본문 끝이나 꼬리말을 넘어도 행을 다음 쪽으로 넘기지
    // 않는다 — table-ipc/table-complex(본문 안, 한컴 기준 PDF)·온새미로 표지 틀(꼬리말 영역까지,
    // 한컴 2020·2024: 다음 쪽 본문 무이동). 이 식을 용지 기준 표에 쓰면 본문 시작만큼
    // 표가 아래에 있다고 오판해 본문 안의 표를 잘랐고, 잘린 행은 다음 쪽 표에 가려 사라졌다.
    // 쪽 기준 표도 본문 영역 기준의 절대 위치라 같은 규칙을 따른다(한컴 출력 미확인).
    if !matches!(
        table.common.vert_rel_to,
        crate::model::shape::VertRelTo::Para
    ) {
        return None;
    }
    let anchor_y = current_height + hwpunit_to_px(table.common.vertical_offset as i32, dpi);
    let room = base_available_height() - anchor_y;
    if room > 0.0 && ft.effective_height > room {
        // `cumulative_heights` 는 접두합(len = 행 수 + 1)이다 —
        // `cum[i]` 는 행 0..i 의 합이므로, 처음으로 room 을 넘는
        // 인덱스 i 는 "행 i-1 이 안 들어간다"는 뜻이다.
        let first_unfit = ft
            .cumulative_heights
            .iter()
            .position(|cum| *cum > room)
            .map(|i| i.saturating_sub(1))
            .unwrap_or(ft.row_heights.len());
        if first_unfit > 0 && first_unfit < ft.row_heights.len() {
            let row_room = room - ft.cumulative_heights[first_unfit] - ft.cell_spacing;
            // 저장 줄 없는 행의 글이 남은 자리에 다 들어가면 한/글은 그 행을 나누지 않고 이 쪽에
            // 둔다. 선언 높이 가운데 본문 아래끝을 넘는 빈 몫은 깎아 버린다(한컴 2020 Print:
            // 표 아래선이 본문 아래끝에 닿는다). 넘길 글이 없으므로 그 행 때문에 쪽을 열지 않는다.
            if whole_row_fits(first_unfit, row_room) {
                let remaining_px = (ft.effective_height
                    - ft.cumulative_heights
                        .get(first_unfit + 1)
                        .copied()
                        .unwrap_or(ft.effective_height))
                .max(0.0);
                return Some(OverlayContinuation {
                    first_unfit,
                    remaining_px,
                    reserve_px: 0.0,
                    room,
                    // 행 컷 없이 표를 그리되 표 위끝에서 본문 아래끝까지(room)로 자른다.
                    row_fragment: Some(OverlayRowFragment {
                        end_cut: Vec::new(),
                        opening_height: room,
                        remaining_height: 0.0,
                        owns_stored_frame: false,
                    }),
                    kept_whole_row: true,
                });
            }
            let row_fragment = partial_row(first_unfit, row_room);
            let remaining_px = (ft.effective_height
                - ft.cumulative_heights
                    .get(first_unfit)
                    .copied()
                    .unwrap_or(0.0))
            .max(0.0);
            let remaining_px = row_fragment.as_ref().map_or(remaining_px, |fragment| {
                remaining_px - ft.row_heights[first_unfit] - ft.cell_spacing
                    + fragment.remaining_height
            });
            // [#5792] 잔여 행이 놓일 자리를 뒤따르는 흐름이 스스로
            // 만드는가? #4514 형상은 앵커 뒤 빈 필러 문단들이 표
            // 높이만큼 흐름을 만들므로(저장 사다리가 앵커 → 필러로
            // 연속 전진) 다음 쪽에 잔여 높이를 다시 예약하면 이중
            // 계상이다. 반대로 뒤 문단의 저장 vpos 가 앵커보다
            // **되감기면**(쪽 리셋) 그 문단은 새 쪽 상단에서 다시
            // 시작하는 좌표라 잔여 행의 자리가 어디에도 없다. 그때
            // 예약하지 않으면 다음 쪽 본문이 잔여 행 위에 겹쳐
            // 그려지고(2700727 3쪽 'Ⅱ. 곤충이용'·'1. 설치기준'),
            // 그 본문 표가 잔여 행의 페인트 상한을 깎아 행이 통째로
            // 사라진다(42행 중 17행 소실).
            let ladder_resets_after_anchor = next_para
                .and_then(|np| np.line_segs.first().map(|seg| seg.vertical_pos))
                .zip(para.line_segs.first().map(|seg| seg.vertical_pos))
                .is_some_and(|(next_vpos, anchor_vpos)| next_vpos < anchor_vpos);
            let reserve_px = if ladder_resets_after_anchor {
                remaining_px
            } else {
                0.0
            };
            return Some(OverlayContinuation {
                first_unfit,
                remaining_px,
                reserve_px,
                room,
                row_fragment,
                kept_whole_row: false,
            });
        }
    }
    None
}

pub(super) fn whole_row_fits_query(
    engine: &crate::renderer::typeset::TypesetEngine,
    state: &crate::renderer::typeset::TypesetState,
    table: &Table,
    formatted: &FormattedTable,
    styles: &crate::renderer::style_resolver::ResolvedStyleSet,
    row: usize,
    available: f64,
) -> bool {
    row_fragment::whole_row_fits(engine, state, table, formatted, styles, row, available)
}

pub(super) fn partial_row_query(
    engine: &crate::renderer::typeset::TypesetEngine,
    state: &crate::renderer::typeset::TypesetState,
    table: &Table,
    formatted: &FormattedTable,
    styles: &crate::renderer::style_resolver::ResolvedStyleSet,
    row: usize,
    available: f64,
) -> Option<OverlayRowFragment> {
    row_fragment::query(engine, state, table, formatted, styles, row, available)
}
