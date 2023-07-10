use std::cmp::Ordering;

pub fn slide(
    pos: (usize, usize),
    hole: (usize, usize),
    mut f: impl FnMut((usize, usize), (usize, usize)),
) -> bool {
    let (row_cmp, col_cmp) = (pos.0.cmp(&hole.0), pos.1.cmp(&hole.1));

    match (row_cmp, col_cmp) {
        (Ordering::Equal, Ordering::Less) => {
            for i in (pos.1..hole.1).rev() {
                f((pos.0, i), (pos.0, i + 1));
            }
        }
        (Ordering::Equal, Ordering::Greater) => {
            for i in hole.1..pos.1 {
                f((pos.0, i + 1), (pos.0, i));
            }
        }
        (Ordering::Less, Ordering::Equal) => {
            for i in (pos.0..hole.0).rev() {
                f((i, pos.1), (i + 1, pos.1));
            }
        }
        (Ordering::Greater, Ordering::Equal) => {
            for i in hole.0..pos.0 {
                f((i + 1, pos.1), (i, pos.1));
            }
        }

        _ => return false,
    };

    true
}
