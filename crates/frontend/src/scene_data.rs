use std::collections::HashMap;

pub(crate) const CELL_VALUES: usize = 12;

/// Resolve stable IDs once per snapshot, then upload one instance per bond.
/// Layout: ax, ay, bx, by, id_a, id_b (six float32 values).
pub(crate) fn pack_links(cells: &[f32], links: &[u32]) -> Result<Vec<f32>, &'static str> {
    if cells.len() % CELL_VALUES != 0 || links.len() % 2 != 0 {
        return Err("Invalid bond snapshot length");
    }
    if links.is_empty() {
        return Ok(Vec::new());
    }
    let mut positions = HashMap::with_capacity(cells.len() / CELL_VALUES);
    for cell in cells.chunks_exact(CELL_VALUES) {
        let raw = cell[0];
        if !cell[1].is_finite() || !cell[2].is_finite() {
            return Err("Invalid cell ID or position in bond snapshot");
        }
        // Negative IDs are pending local spawns ("ghosts"). They are part of
        // the scene before the server confirms them, but they never own bonds.
        if raw < 0. {
            if raw.fract() != 0. {
                return Err("Invalid cell ID or position in bond snapshot");
            }
            continue;
        }
        let id = raw as u32;
        if id == 0 || id as f32 != raw || positions.insert(id, (cell[1], cell[2])).is_some() {
            return Err("Invalid cell ID or position in bond snapshot");
        }
    }
    let mut packed = Vec::with_capacity(links.len() * 3);
    for pair in links.as_chunks::<2>().0 {
        if pair[0] == pair[1] {
            return Err("Self bond in snapshot");
        }
        let &(ax, ay) = positions.get(&pair[0]).ok_or("Missing bond endpoint")?;
        let &(bx, by) = positions.get(&pair[1]).ok_or("Missing bond endpoint")?;
        packed.extend([ax, ay, bx, by, pair[0] as f32, pair[1] as f32]);
    }
    Ok(packed)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cells() -> Vec<f32> {
        vec![
            9., 40., 50., 10., 0., 1., 0., 0., 0., 0., -1., 0., 2., 10., 20., 10., 2., 1., 0., 0.,
            0., 0., -1., 0.,
        ]
    }
    #[test]
    fn resolves_ids_independently_of_snapshot_order() {
        assert_eq!(
            pack_links(&cells(), &[2, 9]).unwrap(),
            vec![10., 20., 40., 50., 2., 9.]
        );
    }
    #[test]
    fn accepts_empty_world_and_removal_of_last_bond() {
        assert!(pack_links(&[], &[]).unwrap().is_empty());
        assert!(pack_links(&cells(), &[]).unwrap().is_empty());
    }

    #[test]
    fn ignores_unlinked_pending_spawn_ghosts() {
        let mut c = cells();
        c.extend([-1., 100., 100., 10., 0., 1., 0., 0., 0., 0., -1., 0.]);
        assert_eq!(
            pack_links(&c, &[2, 9]).unwrap(),
            vec![10., 20., 40., 50., 2., 9.]
        );
        assert!(pack_links(&c, &[u32::MAX, 9]).is_err());
        let ghost = c.len() - CELL_VALUES;
        c[ghost] = -1.5;
        assert!(pack_links(&c, &[2, 9]).is_err());
    }
    #[test]
    fn rejects_dangling_self_and_incomplete_bonds() {
        for edges in [vec![2, 3], vec![2, 2], vec![2]] {
            assert!(pack_links(&cells(), &edges).is_err());
        }
        assert!(pack_links(&cells()[1..], &[2, 9]).is_err());
    }
    #[test]
    fn rejects_ambiguous_ids_and_invalid_positions() {
        let mut c = cells();
        c[0] = 2.;
        assert!(pack_links(&c, &[2, 9]).is_err());
        c[0] = 9.;
        c[1] = f32::NAN;
        assert!(pack_links(&c, &[2, 9]).is_err());
    }
}
