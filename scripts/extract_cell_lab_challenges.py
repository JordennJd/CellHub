#!/usr/bin/env python3
"""Extract Cell Lab v103 challenge data (manifests, substrate settings, initial
genomes) from an Android APK into documentation files.

The APK ships its challenge campaign as two kinds of assets:

* ``assets/levels.xml`` — the challenge manifest: per challenge id, substrate
  file, win/lose conditions, tools, rewards;
* ``assets/<name>.substrate`` — one Java-serialized "petri dish" per challenge:
  substrate settings (Environment), initial cells with full genomes, and food.

The binary format below mirrors, read for read, the decompiled v103 classes
``Environment``, ``Cell``, ``Gene`` and ``Link`` of ``com.saterskog.cell_lab``.

Usage:
    python3 -I scripts/extract_cell_lab_challenges.py <apk> <output-dir>

Outputs into <output-dir>:
    levels.xml    verbatim copy of the original manifest
    index.json    merged manifest + per-challenge summary
    data/<id>.json  full decoded substrate of one challenge
    challenges.md   human-readable rendering (Russian)
"""

from __future__ import annotations

import argparse
import io
import json
import math
import struct
import sys
import zlib
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Java ObjectInputStream-compatible reader
#
# A serialized experiment starts with the stream header (AC ED 00 05) followed
# by TC_BLOCKDATA chunks; primitive reads cross block boundaries transparently.
# The cell/food section is a second ObjectOutputStream wrapped around a
# GZIP stream, which repeats the header.


class JavaStreamReader:
    """Big-endian primitive reader over Java serialization block data."""

    def __init__(self, data: bytes) -> None:
        self._src = io.BytesIO(data)
        header = self._src.read(4)
        if header != b"\xac\xed\x00\x05":
            raise ValueError(f"not a Java serialization stream: {header.hex()}")
        self._block = io.BytesIO()

    def _need(self, count: int) -> None:
        while len(self._block.getbuffer()) - self._block.tell() < count:
            marker = self._src.read(1)
            if not marker:
                raise EOFError("unexpected end of stream")
            kind = marker[0]
            if kind == 0x77:  # TC_BLOCKDATA
                length = self._src.read(1)[0]
            elif kind == 0x7A:  # TC_BLOCKDATALONG
                length = int.from_bytes(self._src.read(4), "big")
            else:
                raise ValueError(f"unexpected stream marker 0x{kind:02x}")
            rest = self._block.getbuffer()[self._block.tell():].tobytes()
            self._block = io.BytesIO(rest + self._src.read(length))

    def raw(self, count: int) -> bytes:
        self._need(count)
        return self._block.read(count)

    def rest(self) -> bytes:
        """All remaining underlying bytes (used to locate the gzip blob)."""
        self._block.seek(0, io.SEEK_END)
        return self._src.read()

    def boolean(self) -> bool:
        return self.raw(1)[0] != 0

    def int32(self) -> int:
        return int.from_bytes(self.raw(4), "big", signed=True)

    def int16(self) -> int:
        return int.from_bytes(self.raw(2), "big", signed=True)

    def float32(self) -> float:
        return struct.unpack(">f", self.raw(4))[0]

    def float64(self) -> float:
        return struct.unpack(">d", self.raw(8))[0]


# ---------------------------------------------------------------------------
# Cell type enum `h` from the v103 bytecode, in declaration (= ordinal) order.

CELL_TYPES = [
    "PHAGOCYTE", "FLAGELLOCYTE", "PHOTOCYTE", "DEVOROCYTE", "LIPOCYTE",
    "KERATINOCYTE", "BUOYOCYTE", "GLUEOCYTE", "VIROCYTE", "NITROCYTE",
    "STEREOCYTE", "SENSEOCYTE", "MYOCYTE", "NEUROCYTE", "SECROCYTE",
    "STEMOCYTE", "GAMETE", "CILIOCYTE",
]


def cell_type_name(ordinal: int) -> str:
    if 0 <= ordinal < len(CELL_TYPES):
        return CELL_TYPES[ordinal]
    return f"UNKNOWN({ordinal})"


# ---------------------------------------------------------------------------
# Environment (substrate settings)
#
# Field names follow the decompiled single-letter fields; semantics were
# recovered from the settings sliders (class `o`) and CellWorld usage.


def parse_environment(r: JavaStreamReader) -> dict[str, Any]:
    """Decode ``Environment.a(ObjectInputStream)`` with all version branches."""
    version = r.int32()
    if version > 95 or version <= 0:
        raise ValueError(f"Incompatible environment format {version}")
    env: dict[str, Any] = {"format_version": version}

    env["food"] = r.float64() * (0.15 if version == 1 else 1.0)
    env["food_size"] = r.float64() if version > 1 else 0.15
    env["radiation"] = r.float64()
    env["light"] = r.float64()
    env["light_rotation_speed"] = r.float64()
    env["light_range"] = r.float64()
    if version <= 23:  # legacy slot between light_range and viscosity
        r.float64()
    env["viscosity"] = r.float64()

    # Allowed cell types: writer stored the count of types known when the file
    # was made; v103 knows 18 and fills the rest with false.
    stored_types = r.int32()
    allowed = [r.boolean() for _ in range(stored_types)]
    allowed += [False] * (len(CELL_TYPES) - min(stored_types, len(CELL_TYPES)))
    env["allowed_cell_types"] = [
        name for i, name in enumerate(CELL_TYPES) if allowed[i]
    ]

    env["add_random_cells"] = r.boolean()
    env["gravity"] = r.float64()
    env["density"] = r.float64()
    env["gradient"] = r.float64()
    env["die_at_edge"] = r.boolean()

    if version > 20:
        env["nitrates"] = r.float64()
    else:
        env["nitrates"] = 1.0 if r.boolean() else 0.0

    if version < 3:
        env["max_cells"] = 800
        env["legacy_s"] = 3500  # stored on newer files, unused by v103
        env["substrate_radius"] = 1.0
    else:
        env["max_cells"] = r.int32()
        env["legacy_s"] = r.int32()
        env["substrate_radius"] = r.float64()

    if version < 9:
        env["friction_dynamic"] = 0.0
        env["friction_static"] = 0.0
        env["tweak"] = False
    else:
        env["friction_dynamic"] = r.float64()
        env["friction_static"] = r.float64()
        env["tweak"] = r.boolean()

    env["salinity"] = 0.3 if version < 10 else r.float32()

    if version >= 14:
        env["aging"] = r.boolean()
        env["food_lump"] = r.float64()
        env["food_lump_size"] = r.float64()
    else:
        env["aging"] = True
        env["food_lump"] = 0.0
        env["food_lump_size"] = 0.01

    if version >= 22:
        env["mobile_food"] = r.boolean()
        env["food_layer"] = r.float32()
    else:
        env["mobile_food"] = False
        env["food_layer"] = 0.0

    return env


# ---------------------------------------------------------------------------
# Gene (one genome mode)
#
# Static calibration tables from Gene.<clinit>:
#   w (u[i] option counts) = [40,40,4,6,15,4,4,4,4,20,6]
#   x[0]=0, y[0]=1  (legacy migration normalization)
#   v[i] ranges z/A: v0 ±60, v1..v3 0..1, v4 0..√3, v5 0..0.01, v6 0.5..2

TWO_PI = 2.0 * math.pi


def parse_gene(r: JavaStreamReader, cell_version: int) -> dict[str, Any]:
    """Decode ``Gene.a(ObjectInputStream, int)`` with all version branches."""
    gene = r.int32() if cell_version > 2 else 1
    g: dict[str, Any] = {"format_version": gene}

    if gene < 7:  # legacy: doubles on disk, linear priority
        color = [r.float32() for _ in range(3)]
        split_mass = r.float64()
        child2_ratio = r.float64()
        split_angle = r.float64()
        child1_angle = r.float64() + (math.pi if gene == 5 else 0.0)
        child2_angle = r.float64()
        # gene < 9 always holds in this branch
        split_angle += math.pi
        child1_angle += math.pi
        child2_angle += math.pi
        priority = r.float64()
        child1_mode = r.int32()
        child2_mode = r.int32()
        make_adhesin = r.boolean()
        keep_adhesin_child1 = r.boolean()
        keep_adhesin_child2 = r.boolean()
        cell_type = r.int32()
        legacy_k = r.int32()
        stay_alive = r.boolean()
        migration_coeff = r.float64()  # "f4": feeds migrated channel 1
        legacy_mode1 = legacy_mode2 = 0
        is_primary = mirror_child1 = mirror_child2 = False
        stiffness = 0.0
        if gene >= 3:
            legacy_mode1 = r.int32()
            legacy_mode2 = r.int32()
        if gene >= 4:
            is_primary = r.boolean()
            mirror_child1 = r.boolean()
            mirror_child2 = r.boolean()
            stiffness = r.float64()  # note: no ×35 scaling in this branch
        channel_hint = 0.4  # "f3": gene < 8 is not stored
    else:  # modern: floats on disk
        color = [r.float32() for _ in range(3)]
        split_mass = r.float32()
        child2_ratio = r.float32()
        split_angle = r.float32()
        child1_angle = r.float32()
        child2_angle = r.float32()
        if gene < 9:
            split_angle += math.pi
            child1_angle += math.pi
            child2_angle += math.pi
            child2_ratio = 1.0 - child2_ratio
        priority = r.float32()
        child1_mode = r.int32()
        child2_mode = r.int32()
        make_adhesin = r.boolean()
        keep_adhesin_child1 = r.boolean()
        keep_adhesin_child2 = r.boolean()
        cell_type = r.int32()
        legacy_k = r.int32()
        stay_alive = r.boolean()
        migration_coeff = 0.0
        legacy_mode1 = legacy_mode2 = 0
        if gene < 9:  # values only used by the migration path below
            migration_coeff = r.float32()
            legacy_mode1 = r.int32()
            legacy_mode2 = r.int32()
        is_primary = r.boolean()
        mirror_child1 = r.boolean()
        mirror_child2 = r.boolean()
        stiffness = r.float32() * 35.0 if gene < 16 else r.float32()
        if gene < 8:
            channel_hint = 0.4
        elif gene == 8:
            channel_hint = r.float32()
        else:
            channel_hint = 0.0

    g.update(
        cell_type=cell_type_name(cell_type),
        color_rgb=color,
        split_mass=split_mass,
        child2_mass_ratio=child2_ratio,
        split_angle_rad=split_angle % TWO_PI,
        child1_angle_rad=child1_angle % TWO_PI,
        child2_angle_rad=child2_angle % TWO_PI,
        priority=priority,
        stiffness=stiffness,
        make_adhesin=make_adhesin,
        keep_adhesin_child1=keep_adhesin_child1,
        keep_adhesin_child2=keep_adhesin_child2,
        stay_alive=stay_alive,
        mirror_child1=mirror_child1,
        mirror_child2=mirror_child2,
        is_primary=is_primary,
        child1_mode=child1_mode if 0 <= child1_mode < 40 else 0,
        child2_mode=child2_mode if 0 <= child2_mode < 40 else 0,
    )
    if legacy_k:
        g["legacy_k"] = legacy_k

    if gene >= 9:
        # 11 signaling channels: substance (short), transfer function
        # (short for gene >= 17, otherwise derived from substance),
        # then three float coefficients.
        channels = []
        for i in range(11):
            substance = r.int16()
            if gene >= 17:
                transfer = r.int16()
            elif substance == -1:
                substance, transfer = 0, 0
            elif substance < 4:
                transfer = 1
            else:
                substance -= 4
                transfer = 2
            channels.append(
                {
                    "channel": i,
                    "substance": substance,
                    "transfer": transfer,
                    "a": r.float32(),
                    "b": r.float32(),
                    "c": r.float32(),
                }
            )
        outputs = []
        for i in range(11):
            if gene == 10 and i == 4:
                outputs.append(1)
            elif gene < 15 and i == 9:
                outputs.append(18)
            elif gene >= 20 or i != 10:
                outputs.append(r.int32())
            else:
                outputs.append(0)
        params = []
        for i in range(7):
            if gene < 14 and i == 6:
                params.append(1.0)
            elif gene >= 16 or i != 5:
                params.append(r.float32())
            else:
                params.append(0.0)
        g["channels"] = channels
        g["outputs"] = outputs
        g["type_params"] = params
    else:
        # Reader-side migration to the channel model; nothing on disk.
        # t[0].b = 2*f3-1, t[1].b = 2*f2-1 (x[0]=0, y[0]=1), v[6]=1,
        # u[0]=mode1, u[1]=mode2, u[9]=w[9]-1=19.
        g["channels"] = [
            {"channel": 0, "substance": 0, "transfer": 0,
             "a": 0.0, "b": 2.0 * channel_hint - 1.0, "c": 0.0},
            {"channel": 1, "substance": 0, "transfer": 0,
             "a": 0.0, "b": 2.0 * migration_coeff - 1.0, "c": 0.0},
        ]
        g["outputs"] = [legacy_mode1 if i == 0
                        else legacy_mode2 if i == 1
                        else 19 if i == 9 else 0
                        for i in range(11)]
        g["type_params"] = [1.0 if i == 6 else 0.0 for i in range(7)]
        g["migrated_from_legacy"] = True

    return g


# ---------------------------------------------------------------------------
# Link and Cell


def parse_link(r: JavaStreamReader, cell_version: int) -> dict[str, Any]:
    link: dict[str, Any] = {
        "mode": r.int32(),
        "x": r.float64(),
        "y": r.float64(),
        "dead": r.boolean(),
        "dx": r.float64(),
        "dy": r.float64(),
    }
    if cell_version >= 7:
        link["stiffness"] = r.float32()
    elif cell_version >= 4:
        link["stiffness"] = r.float64()
    else:
        link["stiffness"] = 0.0
    link["length_factor"] = r.float32() if cell_version >= 16 else 0.0
    return link


def parse_cell(r: JavaStreamReader) -> dict[str, Any]:
    """Decode ``Cell.a(ObjectInputStream)`` with all version branches."""
    version = r.int32()
    if version < 2 or version > 95:
        raise ValueError(f"bad cell format version {version}")
    cell: dict[str, Any] = {"format_version": version}
    cell["pos_x"] = r.float64()   # b
    cell["pos_y"] = r.float64()   # c
    cell["angle"] = r.float64()   # k
    cell["rotation"] = r.float64()  # m
    cell["vel_x"] = r.float64()   # e
    cell["vel_y"] = r.float64()   # f
    cell["angular_velocity"] = r.float64()  # l
    cell["angular_position"] = r.float64()  # n
    cell["radius"] = r.float64()  # d
    cell["mass"] = r.float64()    # w
    cell["age_h"] = r.float64()   # o

    link_count = r.int32()
    stored = min(link_count, 20)
    cell["links"] = [parse_link(r, version) for _ in range(stored)]
    for _ in range(link_count - stored):
        parse_link(r, version)  # over the 20-slot array: decode and drop
    cell["link_count"] = link_count

    cell["mode_timer"] = r.int32()  # C
    cell["dead"] = r.boolean()      # x
    cell["color_rgb"] = [r.float32() for _ in range(3)]  # J

    gene_count = r.int32()
    stored_genes = min(gene_count, 40)
    genes = [parse_gene(r, version) for _ in range(stored_genes)]
    for _ in range(gene_count - stored_genes):
        parse_gene(r, version)  # over the 40-slot array: decode and drop
    # Slots gene_count..39 hold a copy of gene 0 with p=false; not stored here.
    cell["genome_modes"] = gene_count
    cell["genes"] = genes

    cell["primary_mode"] = r.int32()   # D
    cell["owner_class"] = r.int32()    # E: 0 level, 1 user, 3 infected, 6 hybrid
    cell["state"] = r.int32()          # F
    cell["t"] = r.float64()
    cell["u"] = r.float64()
    cell["v"] = r.float64()
    cell["p"] = r.float64()
    if version >= 4:
        cell["y"] = r.boolean()

    if version >= 9:
        # U[i]/W[i] substrate uptake/output arrays
        cell["substrate_uptake"] = [r.float32() for _ in range(4)]
        cell["substrate_output"] = [r.float32() for _ in range(4)]
        cell["x"] = r.float32()
        cell["y2"] = r.float32()
        cell["aa"] = r.float32()
        cell["ab"] = r.float32()
        cell["ac"] = r.float32()
        cell["s"] = r.float32() if version > 11 else 0.0
    else:
        cell["substrate_uptake"] = [0.01] * 4
        cell["substrate_output"] = [0.01] * 4
        cell["s"] = 0.0

    cell["g"] = r.int32() if version >= 15 else 0
    cell["tag"] = r.int32() if version >= 19 else -1
    cell["q"] = r.float64() if version >= 23 else 0.0
    return cell


# ---------------------------------------------------------------------------
# Whole substrate file


def _clean(value: Any) -> Any:
    """Drop internal keys and round floats for stable, compact JSON."""
    if isinstance(value, float):
        return round(value, 6)
    if isinstance(value, list):
        return [_clean(v) for v in value]
    if isinstance(value, dict):
        return {k: _clean(v) for k, v in value.items() if k != "legacy_s"}
    return value


def parse_substrate(data: bytes) -> dict[str, Any]:
    """Decode one ``.substrate`` asset (byte layout: see substrate-format.md)."""
    r = JavaStreamReader(data)
    sub: dict[str, Any] = {}
    sub["format_version"] = r.int32()
    sub["time_h"] = r.float64()  # M: initial simulation time, hours
    cell_count = r.int32()       # z
    sub["environment"] = parse_environment(r)

    body = r
    if sub["format_version"] > 12:
        # New ObjectOutputStream over GZIP, starting with its own AC ED header.
        # The shipped blobs are truncated (no gzip end marker): the game only
        # ever reads the promised cell/food records and never sees the trailer,
        # so we inflate tolerantly and parse exactly what the header promises.
        inflater = zlib.decompressobj(16 + zlib.MAX_WBITS)
        inflated = inflater.decompress(r.rest())
        try:
            inflated += inflater.flush()
        except zlib.error:
            pass
        body = JavaStreamReader(inflated)

    if sub["format_version"] > 23:
        sub["initial_light_phase"] = body.float64()  # O

    sub["cells"] = [parse_cell(body) for _ in range(cell_count)]
    sub["cell_count"] = cell_count

    food_count = body.int32()  # B
    sub["food_count"] = food_count
    food = []
    for _ in range(food_count):
        chunk = ([body.float32() for _ in range(3)] if sub["format_version"] >= 8
                 else [body.float64() for _ in range(3)])
        if sub["format_version"] >= 22:  # initial velocity
            chunk += [body.float32() for _ in range(3)]
        food.append(chunk)
    sub["food"] = food
    return _clean(sub)


# Per-cell fields that are zero/default in every shipped challenge file; the
# full parser keeps them (see parse_cell), the compact view drops them unless
# a file actually differs from the default.
CELL_DEFAULTS = {
    "rotation": 0.0, "vel_x": 0.0, "vel_y": 0.0,
    "angular_velocity": 0.0, "angular_position": 0.0,
    "dead": False, "state": 0, "t": 0.0, "u": 0.0, "v": 0.0, "p": 0.0,
    "y": False, "s": 0.0, "x": 0.0, "y2": 0.0, "aa": 0.0, "ab": 0.0,
    "ac": 0.0, "g": 0, "tag": -1, "q": 0.0,
}
CELL_SUBSTRATE_DEFAULT = [0.01, 0.01, 0.01, 0.01]


def compact_substrate(sub: dict[str, Any]) -> dict[str, Any]:
    """Reduce a parsed substrate for storage.

    Shipped files repeat the same genome for hundreds of initial cells, so
    genomes are deduplicated into ``genomes`` and referenced by index. The
    full per-cell layout remains documented in substrate-format.md.
    """
    cell_versions = {c["format_version"] for c in sub["cells"]}
    genomes: list[list[dict[str, Any]]] = []
    genome_index: dict[str, int] = {}
    cells = []
    for c in sub["cells"]:
        extras = {k: c[k] for k in CELL_DEFAULTS
                  if k in c and c[k] != CELL_DEFAULTS[k]}
        for key, default in (("substrate_uptake", CELL_SUBSTRATE_DEFAULT),
                             ("substrate_output", CELL_SUBSTRATE_DEFAULT)):
            if key in c and c[key] != default:
                extras[key] = c[key]
        if c["link_count"] > len(c["links"]):
            extras["links_beyond_20"] = c["link_count"]
        key = json.dumps(c["genes"], sort_keys=True)
        if key not in genome_index:
            genome_index[key] = len(genomes)
            genomes.append(c["genes"])
        cell = {
            "pos": [c["pos_x"], c["pos_y"]],
            "angle": c["angle"],
            "radius": c["radius"],
            "mass": c["mass"],
            "age_h": c["age_h"],
            "owner_class": c["owner_class"],
            "primary_mode": c["primary_mode"],
            "mode_timer": c["mode_timer"],
            "color_rgb": c["color_rgb"],
            "links": c["links"],
            "genome": genome_index[key],
        }
        if extras:
            cell["non_default"] = extras
        cells.append(cell)
    out = dict(sub)
    out["cells"] = cells
    out["genomes"] = genomes
    if len(cell_versions) == 1:
        out["cell_format_version"] = cell_versions.pop()
    return out


# ---------------------------------------------------------------------------
# resources.arsc: read a named string-array without apktool
#
# Minimal reader for RES_TABLE (0x0002) → string pool (0x0001) →
# RES_TABLE_PACKAGE (0x0200) → RES_TABLE_TYPE (0x0201) chunks.


def _arsc_varint(data: bytes, pos: int) -> tuple[int, int]:
    b = data[pos]
    if b & 0x80:
        return ((b & 0x7F) << 8) | data[pos + 1], 2
    return b, 1


def _parse_string_pool(data: bytes, off: int) -> list[str]:
    chunk_type, header_size, chunk_size = struct.unpack_from("<HHI", data, off)
    if chunk_type != 0x0001:
        raise ValueError(f"expected string pool chunk at {off:#x}, got {chunk_type:#x}")
    string_count, _style_count, flags, strings_start, _styles_start = struct.unpack_from(
        "<IIIII", data, off + 8
    )
    utf8 = bool(flags & 0x100)
    offsets = struct.unpack_from(f"<{string_count}I", data, off + 28)
    base = off + strings_start
    strings = []
    for offset in offsets:
        pos = base + offset
        if utf8:
            _, skip = _arsc_varint(data, pos)  # utf-16 length, ignored
            pos += skip
            byte_len, skip = _arsc_varint(data, pos)
            pos += skip
            strings.append(data[pos : pos + byte_len].decode("utf-8"))
        else:
            # UTF-16 pool: length is a plain little-endian uint16 (0x8000
            # continues into a second word for very long strings).
            char_len = int.from_bytes(data[pos : pos + 2], "little")
            pos += 2
            if char_len & 0x8000:
                char_len = ((char_len & 0x7FFF) << 16) | int.from_bytes(
                    data[pos : pos + 2], "little"
                )
                pos += 2
            strings.append(data[pos : pos + 2 * char_len].decode("utf-16-le"))
    return strings


def extract_string_array(arsc: bytes, array_name: str) -> list[str]:
    """Return the items of a string-array resource (default config)."""
    table_type, table_header, _table_size, package_count = struct.unpack_from(
        "<HHII", arsc, 0
    )
    if table_type != 0x0002:
        raise ValueError(f"not a resources.arsc table: {table_type:#x}")
    off = table_header
    global_pool = _parse_string_pool(arsc, off)
    off += struct.unpack_from("<HHI", arsc, off)[2]

    result = None
    for _ in range(package_count):
        pkg_type, pkg_header, pkg_size = struct.unpack_from("<HHI", arsc, off)
        if pkg_type != 0x0200:
            raise ValueError(f"expected package chunk at {off:#x}, got {pkg_type:#x}")
        # ResTable_package: header(8), id(4), name(char16[128]=256B),
        # typeStrings(+268), lastPublicType(+272), keyStrings(+276)
        type_pool_off = struct.unpack_from("<I", arsc, off + 268)[0]
        key_pool_off = struct.unpack_from("<I", arsc, off + 276)[0]
        type_pool = _parse_string_pool(arsc, off + type_pool_off)
        key_pool = _parse_string_pool(arsc, off + key_pool_off)

        pos = off + pkg_header
        end = off + pkg_size
        while pos < end:
            chunk_type, chunk_header, chunk_size = struct.unpack_from("<HHI", arsc, pos)
            if chunk_type == 0x0201:  # RES_TABLE_TYPE: entries of one type/config
                type_index = arsc[pos + 8]
                entry_count, entries_start = struct.unpack_from("<II", arsc, pos + 12)
                if type_pool[type_index - 1] != "array":
                    pos += chunk_size
                    continue
                offsets = struct.unpack_from(f"<{entry_count}I", arsc, pos + chunk_header)
                for entry_offset in offsets:
                    if entry_offset == 0xFFFFFFFF:
                        continue
                    entry = pos + entries_start + entry_offset
                    _size, flags, key = struct.unpack_from("<HHI", arsc, entry)
                    if key_pool[key] != array_name or not flags & 0x0001:
                        continue  # not our name / not a bag (array)
                    _parent, map_count = struct.unpack_from("<II", arsc, entry + 8)
                    items = []
                    map_pos = entry + 16
                    for _ in range(map_count):
                        _name = struct.unpack_from("<I", arsc, map_pos)[0]
                        value_size, _res0, value_type, value_data = struct.unpack_from(
                            "<HBBI", arsc, map_pos + 4
                        )
                        if value_type == 0x03:  # STRING
                            items.append(global_pool[value_data])
                        map_pos += 4 + value_size
                    if items:
                        result = items
            pos += chunk_size
        off += pkg_size

    if result is None:
        raise ValueError(f"string-array {array_name!r} not found in resources.arsc")
    return result


# ---------------------------------------------------------------------------
# levels.xml manifest


# Win/lose condition tags of levels.xml with their exact checker semantics
# (decompiled ``j.a(int, CellWorld)``). ``Cell.E`` (owner class): 0 level cell,
# 1 user cell, 3 infected, 6 hybrid. Color counters only consider level cells
# (E == 0) using the color of their current primary mode.
CONDITION_SEMANTICS = {
    "user-cells": "win: count(E=1) >= N",
    "user-cells-max": "lose: count(E=1) > N",
    "level-cells": "win: count(E=0) + count(E=3) <= N",
    "level-cells-min": "win: count(E=0) + count(E=3) >= N",
    "total-cells-min": "win: count(all cells) >= N",
    "infected-level-cells": "win: count(E=3) >= N",
    "infected-level-cells-max": "win: count(E=3) <= N",
    "hybrid-cells-min": "win: count(E=6) >= N",
    "black-level-cells": "win: count(black E=0) <= N (|R|+|G|+|B| < 0.1)",
    "red-level-cells": "win: count(red E=0) <= N (R>0.65, G<0.35, B<0.35)",
    "red-level-cells-min": "win: count(red E=0) >= N",
    "yellow-level-cells": "win: count(yellow E=0) <= N (R>0.65, G>0.65, B<0.35)",
    "yellow-level-cells-min": "win: count(yellow E=0) >= N",
    "win-time": "win: условие должно непрерывно держаться N часов",
    "max-time": "lose: время симуляции превысило N часов",
}

OWNER_CLASS_NAMES = {0: "level", 1: "user", 3: "infected", 6: "hybrid"}


def parse_levels_xml(xml_text: str) -> list[dict[str, str]]:
    """Return raw tag -> value mapping per <level>, document order preserved."""
    levels = []
    for element in ET.fromstring(xml_text).iter("level"):
        levels.append({child.tag: (child.text or "").strip() for child in element})
    return levels


def parse_level_strings(items: list[str]) -> list[dict[str, str]]:
    """Split the flat level_strings array (4 strings per level)."""
    if len(items) % 4 != 0:
        raise ValueError("level_strings is not a multiple of 4")
    return [
        {
            "name": items[i],
            "description": items[i + 1],
            "hint1": items[i + 2] or None,
            "hint2": items[i + 3] or None,
        }
        for i in range(0, len(items), 4)
    ]


def parse_tools(raw: str) -> list[str]:
    """Mirror the game parser: absent tag defaults to cell+info."""
    if not raw:
        return ["cell", "info"]
    return [part for part in raw.split(",") if part]


def _num(raw: str) -> int | float | str:
    try:
        return int(raw)
    except ValueError:
        try:
            return float(raw)
        except ValueError:
            return raw


def build_challenge(index: int, raw: dict[str, str], strings: dict[str, str]) -> dict[str, Any]:
    """Merge one levels.xml entry with its resource strings into a record."""
    conditions = {tag: _num(raw[tag]) for tag in CONDITION_SEMANTICS if tag in raw}
    return {
        "order": index,
        "id": raw.get("id", ""),
        "name": strings["name"],
        "description": strings["description"] or None,
        "hints": [h for h in (strings["hint1"], strings["hint2"]) if h],
        "substrate_file": raw.get("file", ""),
        "difficulty": int(raw.get("difficulty", 0)),
        "tools": parse_tools(raw.get("tools", "")),
        "unlocks": [u for u in raw.get("unlocks", "").split(",") if u],
        "feature": int(raw["feature"]) if "feature" in raw else None,
        "conditions": conditions,
        "place_cells_budget": int(raw.get("place-cells", 1)),
        "remove_max": int(raw["remove-max"]) if "remove-max" in raw else None,
        "max_cells_of_type": {
            key[len("max-"):]: int(value)
            for key, value in raw.items()
            if key.startswith("max-")
        },
    }


# ---------------------------------------------------------------------------
# Rendering (Russian, consistent with docs/original-game/)


def _fmt(value: Any) -> str:
    if isinstance(value, float):
        return f"{value:g}"
    if isinstance(value, bool):
        return "да" if value else "нет"
    if value is None:
        return "—"
    return str(value)


ENV_LABELS = [
    ("food", "еда (food)"),
    ("food_size", "размер еды (food size)"),
    ("radiation", "радиация (radiation)"),
    ("light", "свет (light)"),
    ("light_rotation_speed", "скорость вращения света"),
    ("light_range", "диапазон света (light range)"),
    ("viscosity", "вязкость (viscosity)"),
    ("density", "плотность (density)"),
    ("gravity", "гравитация (gravity)"),
    ("gradient", "градиент (gradient)"),
    ("substrate_radius", "радиус чашки (substrate radius)"),
    ("max_cells", "макс. клеток (max cells)"),
    ("friction_dynamic", "трение динамическое"),
    ("friction_static", "трение статическое"),
    ("salinity", "солёность (salinity)"),
    ("food_lump", "комки еды (food lump)"),
    ("food_lump_size", "размер комков"),
    ("food_layer", "слой еды (food layer)"),
    ("nitrates", "нитраты (nitrates)"),
    ("aging", "старение (aging)"),
    ("add_random_cells", "случайные клетки (add random cells)"),
    ("die_at_edge", "смерть у края (die at edge)"),
    ("mobile_food", "подвижная еда (mobile food)"),
    ("tweak", "tweak"),
]


def _genome_table(genome: list[dict[str, Any]]) -> list[str]:
    lines = [
        "| режим | тип | ★ | split mass | split angle° | child1° | child2° | →M1 | →M2 | адгезин | цвет |",
        "|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for mode_index, gene in enumerate(genome):
        color = gene["color_rgb"]
        hex_color = "#%02x%02x%02x" % tuple(
            max(0, min(255, int(round(c * 255)))) for c in color
        )
        lines.append(
            f"| M{mode_index} | {gene['cell_type']} "
            f"| {'★' if gene['is_primary'] else ''} "
            f"| {_fmt(gene['split_mass'])} "
            f"| {_fmt(math.degrees(gene['split_angle_rad']))} "
            f"| {_fmt(math.degrees(gene['child1_angle_rad']))} "
            f"| {_fmt(math.degrees(gene['child2_angle_rad']))} "
            f"| {gene['child1_mode']} | {gene['child2_mode']} "
            f"| {'да' if gene['make_adhesin'] else ''} | {hex_color} |"
        )
    return lines


def render_challenge_md(challenge: dict[str, Any], substrate: dict[str, Any] | None) -> str:
    lines = [f"## {challenge['order']}. {challenge['name']} (id `{challenge['id']}`)", ""]
    meta = [
        f"- Файл: `{challenge['substrate_file']}`",
        f"- Сложность: {challenge['difficulty']}",
        f"- Инструменты: {', '.join(challenge['tools'])}",
        f"- Лимит постановки клеток (place-cells): {challenge['place_cells_budget']}",
    ]
    if challenge["remove_max"] is not None:
        meta.append(f"- Лимит удалений (remove-max): {challenge['remove_max']}")
    if challenge["unlocks"]:
        meta.append(f"- Открывает: {', '.join(challenge['unlocks'])}")
    if challenge["feature"] is not None:
        meta.append(f"- Открывает сервисный флаг: feature {challenge['feature']}")
    if challenge["max_cells_of_type"]:
        meta.append(
            "- Ограничения на режимы генома: "
            + ", ".join(f"{k} ≤ {v}" for k, v in challenge["max_cells_of_type"].items())
        )
    lines += meta + [""]
    if challenge["description"]:
        lines += [challenge["description"], ""]
    for i, hint in enumerate(challenge["hints"], 1):
        lines += [f"**Подсказка {i}.** {hint}", ""]
    if challenge["conditions"]:
        lines += ["**Условия победы/проигрыша** (levels.xml):", ""]
        for tag, value in challenge["conditions"].items():
            lines.append(f"- `{tag}` = {_fmt(value)}")
        lines.append("")

    if substrate is None:
        return "\n".join(lines + ["> Субстрат не удалось декодировать.", ""])

    env = substrate["environment"]
    lines += [
        f"### Субстрат (версия формата {env['format_version']})",
        "",
        "| Параметр | Значение |",
        "|---|---|",
    ]
    for key, label in ENV_LABELS:
        if key in env:
            lines.append(f"| {label} | {_fmt(env[key])} |")
    lines.append("| разрешённые типы клеток | " + ", ".join(env["allowed_cell_types"]) + " |")
    lines.append("")

    lines += [f"### Начальные клетки: {substrate['cell_count']}", ""]
    cells = substrate["cells"]
    genomes = substrate["genomes"]
    if cells:
        shown = cells[:20]
        lines += [
            "| № | x | y | радиус | масса | класс | режим | тип | цвет | геном |",
            "|---|---|---|---|---|---|---|---|---|---|",
        ]
        for i, cell in enumerate(shown):
            genome = genomes[cell["genome"]]
            primary = (
                genome[cell["primary_mode"]]
                if cell["primary_mode"] < len(genome)
                else {}
            )
            color = primary.get("color_rgb", [0, 0, 0])
            hex_color = "#%02x%02x%02x" % tuple(
                max(0, min(255, int(round(c * 255)))) for c in color
            )
            lines.append(
                f"| {i} | {_fmt(cell['pos'][0])} | {_fmt(cell['pos'][1])} "
                f"| {_fmt(cell['radius'])} | {_fmt(cell['mass'])} "
                f"| {OWNER_CLASS_NAMES.get(cell['owner_class'], cell['owner_class'])} "
                f"| M{cell['primary_mode']} | {primary.get('cell_type', '?')} "
                f"| {hex_color} | {cell['genome']} |"
            )
        lines.append("")
        if len(cells) > 20:
            lines += [f"… и ещё {len(cells) - 20} клеток (см. `data/{challenge['id']}.json`).", ""]
        max_tables = 12
        for gi, genome in enumerate(genomes[:max_tables]):
            lines += [f"**Геном №{gi}** (режимов: {len(genome)}):", ""]
            lines += _genome_table(genome)
            lines.append("")
        if len(genomes) > max_tables:
            lines += [
                f"… и ещё {len(genomes) - max_tables} уникальных геномов "
                f"(см. `data/{challenge['id']}.json`, массив `genomes`).",
                "",
            ]

    lines += [
        f"### Еда: {substrate['food_count']} частиц",
        "",
        f"Точные координаты — `data/{challenge['id']}.json`, массив `food`.",
        "",
    ]
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Main


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk", type=Path, help="path to the Cell Lab APK")
    parser.add_argument("output", type=Path, help="directory for the docs")
    parser.add_argument(
        "--max-food", type=int, default=20000, help="food chunks kept per level"
    )
    args = parser.parse_args()

    apk = zipfile.ZipFile(args.apk)
    xml_text = apk.read("assets/levels.xml").decode("utf-8")
    strings = parse_level_strings(
        extract_string_array(apk.read("resources.arsc"), "level_strings")
    )
    raw_levels = parse_levels_xml(xml_text)
    if len(raw_levels) != len(strings):
        print(
            f"warning: {len(raw_levels)} levels but {len(strings)} string blocks",
            file=sys.stderr,
        )

    out = args.output
    (out / "data").mkdir(parents=True, exist_ok=True)
    (out / "levels.xml").write_text(xml_text, encoding="utf-8")

    index = []
    md_parts = [
        "# Cell Lab v103 — кампания (Challenges)",
        "",
        "Кампания оригинальной игры, извлечённая из",
        "`cell_lab_103_android_patched.apk`. Уровень = запись манифеста",
        "(`assets/levels.xml` + строки из `res/values/arrays.xml`) и",
        "сериализованный субстрат (`assets/*.substrate`).",
        "",
        "Машинные данные: `index.json` и `data/<id>.json`; байтовый формат —",
        "см. [substrate-format.md](substrate-format.md); обзор — [README.md](README.md).",
        "",
    ]
    problems = 0
    for i, raw in enumerate(raw_levels):
        challenge = build_challenge(i, raw, strings[i])
        substrate = None
        file_name = challenge["substrate_file"]
        if file_name:
            try:
                substrate = compact_substrate(parse_substrate(apk.read("assets/" + file_name)))
                if len(substrate["food"]) > args.max_food:
                    substrate["food"] = substrate["food"][: args.max_food]
                    substrate["food_truncated"] = True
            except Exception as exc:  # noqa: BLE001 - report and continue
                problems += 1
                print(f"error: {file_name}: {exc}", file=sys.stderr)
        (out / "data" / f"{challenge['id']}.json").write_text(
            json.dumps(
                {"challenge": challenge, "substrate": substrate},
                ensure_ascii=False,
                separators=(",", ":"),
            ),
            encoding="utf-8",
        )
        index.append(
            {
                "order": challenge["order"],
                "id": challenge["id"],
                "name": challenge["name"],
                "substrate_file": file_name,
                "difficulty": challenge["difficulty"],
                "unlocks": challenge["unlocks"],
                "feature": challenge["feature"],
                "conditions": challenge["conditions"],
                "environment_format_version": (
                    substrate["environment"]["format_version"] if substrate else None
                ),
                "cell_count": substrate["cell_count"] if substrate else None,
                "food_count": substrate["food_count"] if substrate else None,
            }
        )
        md_parts.append(render_challenge_md(challenge, substrate))
        md_parts.append("---\n")

    (out / "index.json").write_text(
        json.dumps(
            {"source": "cell_lab_103_android_patched.apk", "challenges": index},
            ensure_ascii=False,
            indent=1,
        ),
        encoding="utf-8",
    )
    (out / "challenges.md").write_text("\n".join(md_parts), encoding="utf-8")
    print(f"extracted {len(index)} challenges to {out} ({problems} errors)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
