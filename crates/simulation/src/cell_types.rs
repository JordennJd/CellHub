//! Stable cell type identifiers for the canonical CellHub genome ABI.

pub(crate) const PHOTOCYTE: u8 = 0;
pub(crate) const PHAGOCYTE: u8 = 1;
pub(crate) const FLAGELLOCYTE: u8 = 2;
pub(crate) const VIROCYTE: u8 = 3;
pub(crate) const KERATINOCYTE: u8 = 4;
pub(crate) const DEVOROCYTE: u8 = 5;
pub(crate) const NITROCYTE: u8 = 6;
pub(crate) const LIPOCYTE: u8 = 7;
pub(crate) const BUOYOCYTE: u8 = 8;
pub(crate) const GLUEOCYTE: u8 = 9;
pub(crate) const STEREOCYTE: u8 = 10;
pub(crate) const SENSEOCYTE: u8 = 11;
pub(crate) const MYOCYTE: u8 = 12;
pub(crate) const NEUROCYTE: u8 = 13;
pub(crate) const SECROCYTE: u8 = 14;
pub(crate) const STEMOCYTE: u8 = 15;
pub(crate) const GAMETE: u8 = 16;
pub(crate) const CILIOCYTE: u8 = 17;

/// Number of implemented archetypes. Mutation cycles and ABI validation use it.
pub(crate) const COUNT: u8 = 18;

/// Types detected by the motile cell sensor.
pub(crate) fn is_motile(kind: u8) -> bool {
    matches!(kind, FLAGELLOCYTE | CILIOCYTE)
}

/// Lipocytes insulate signal diffusion between bonded cells.
pub(crate) fn blocks_signals(kind: u8) -> bool {
    kind == LIPOCYTE
}

/// Keratinocytes shield themselves and their immediate bond neighbours.
pub(crate) fn protects_neighbours(kind: u8) -> bool {
    kind == KERATINOCYTE
}
