use std::collections::HashMap;

/// Resolve stable IDs once per snapshot, then upload one instance per bond.
/// Layout: ax, ay, bx, by, id_a, id_b (six float32 values).
pub(crate) fn pack_links(cells: &[f32], links: &[u32]) -> Result<Vec<f32>, &'static str> {
    if cells.len() % 10 != 0 || links.len() % 2 != 0 {
        return Err("Invalid bond snapshot length");
    }
    if links.is_empty() {
        return Ok(Vec::new());
    }
    let mut positions = HashMap::with_capacity(cells.len() / 10);
    for cell in cells.as_chunks::<10>().0 {
        let id = cell[0] as u32;
        if id == 0
            || id as f32 != cell[0]
            || !cell[1].is_finite()
            || !cell[2].is_finite()
            || positions.insert(id, (cell[1], cell[2])).is_some()
        {
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
            9., 40., 50., 10., 0., 1., 0., 0., 0., 0., 2., 10., 20., 10., 2., 1., 0., 0., 0., 0.,
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
