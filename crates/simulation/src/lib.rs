use hecs::{Entity, World};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use wasm_bindgen::prelude::*;

pub const SIZE: f32 = 3072.;
const GRID: usize = 48;
const STEP: f32 = 1. / 30.;
const LIMIT: usize = 20_000;
const MAX_MODES: usize = 40;
const MODE_STRIDE: usize = 12;
#[derive(Clone, Copy, Debug)]
struct Mode {
    kind: u8,
    threshold: f32,
    child_a: usize,
    child_b: usize,
    ratio: f32,
    speed: f32,
    angle: f32,
    lifespan: f32,
    can_divide: bool,
    bind_children: bool,
    keep_a: bool,
    keep_b: bool,
}
impl Default for Mode {
    fn default() -> Self {
        Self {
            kind: 0,
            threshold: 2.4,
            child_a: 0,
            child_b: 0,
            ratio: 0.5,
            speed: 12.,
            angle: 0.8,
            lifespan: 240.,
            can_divide: true,
            bind_children: false,
            keep_a: false,
            keep_b: false,
        }
    }
}
#[derive(Clone)]
struct Genome(Vec<Mode>);
#[derive(Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
    angle: f32,
}
#[derive(Clone)]
struct Cell {
    id: u32,
    mode: usize,
    mass: f32,
    age: f32,
    generation: u32,
    mutations: u32,
    cooldown: f32,
}
#[derive(Clone, Copy)]
struct Tile {
    light: f32,
    radiation: f32,
    food: f32,
    capacity: f32,
    biome: f32,
}
fn tile_index(x: f32, y: f32) -> usize {
    ((y / 64.) as usize).min(GRID - 1) * GRID + ((x / 64.) as usize).min(GRID - 1)
}
fn map() -> Vec<Tile> {
    (0..GRID * GRID)
        .map(|i| {
            let x = (i % GRID) as f32 / GRID as f32;
            let y = (i / GRID) as f32 / GRID as f32;
            let (light, radiation, capacity, biome) =
                if (x - 0.77).powi(2) + (y - 0.27).powi(2) < 0.035 {
                    (0.35, 0.7, 0.45, 3.)
                } else if y > 0.64 + 0.09 * (x * 12.).sin() {
                    (0.16, 0.02, 1., 2.)
                } else if x < 0.45 + 0.1 * (y * 9.).sin() {
                    (0.95, 0.005, 0.35, 0.)
                } else {
                    (0.5, 0.035, 0.75, 1.)
                };
            Tile {
                light,
                radiation,
                food: capacity,
                capacity,
                biome,
            }
        })
        .collect()
}
fn decode(values: &[f32]) -> Result<Genome, JsError> {
    // v1 used exactly eight modes of eight numbers. v2 uses 1–40 modes of twelve.
    let legacy = values.len() == 64;
    let stride = if legacy { 8 } else { MODE_STRIDE };
    let count = values.len() / stride;
    if count == 0
        || count > MAX_MODES
        || values.len() % stride != 0
        || values.iter().any(|v| !v.is_finite())
    {
        return Err(JsError::new(
            "Genome must contain 1–40 archetypes of 12 finite numbers (or 64 legacy numbers)",
        ));
    }
    let mut modes = vec![Mode::default(); count];
    for (m, v) in modes.iter_mut().zip(values.chunks_exact(stride)) {
        if v[0].fract() != 0.
            || !(0. ..=5.).contains(&v[0])
            || [v[2], v[3]]
                .iter()
                .any(|n| n.fract() != 0. || *n < 0. || *n >= count as f32)
            || (!legacy && v[8..].iter().any(|n| *n != 0. && *n != 1.))
        {
            return Err(JsError::new(
                "Invalid archetype type, child reference or adhesion flag",
            ));
        }
        *m = Mode {
            kind: v[0].clamp(0., 5.) as u8,
            threshold: v[1].clamp(1., 8.),
            child_a: v[2] as usize,
            child_b: v[3] as usize,
            ratio: v[4].clamp(0.2, 0.8),
            speed: v[5].clamp(0., 60.),
            angle: v[6].clamp(-3.14, 3.14),
            lifespan: v[7].clamp(20., 1000.),
            can_divide: legacy || v[8] == 1.,
            bind_children: !legacy && v[9] == 1.,
            keep_a: !legacy && v[10] == 1.,
            keep_b: !legacy && v[11] == 1.,
        };
    }
    Ok(Genome(modes))
}
#[wasm_bindgen]
pub struct Simulation {
    world: World,
    tiles: Vec<Tile>,
    tick: u32,
    next_id: u32,
    rng: u32,
    seed: u32,
    births: u32,
    deaths: u32,
    infections: u32,
    frame: Vec<f32>,
    // Undirected bonds use stable cell IDs, never recycled ECS entity indices.
    bonds: BTreeSet<(u32, u32)>,
}
#[wasm_bindgen]
impl Simulation {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u32) -> Self {
        Self {
            world: World::new(),
            tiles: map(),
            tick: 0,
            next_id: 1,
            rng: seed.max(1),
            seed,
            births: 0,
            deaths: 0,
            infections: 0,
            frame: Vec::new(),
            bonds: BTreeSet::new(),
        }
    }
    pub fn reset(&mut self) {
        *self = Self::new(self.seed);
    }
    pub fn spawn(&mut self, x: f32, y: f32, mode: usize, values: &[f32]) -> Result<u32, JsError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(JsError::new("Invalid spawn"));
        }
        let genome = decode(values)?;
        if mode >= genome.0.len() {
            return Err(JsError::new("Invalid spawn archetype"));
        }
        if self.world.len() as usize >= LIMIT {
            return Err(JsError::new(&format!("Population limit: {LIMIT}")));
        }
        let id = self.next_id;
        self.next_id += 1;
        let angle = self.random() * std::f32::consts::TAU;
        self.world.spawn((
            Position {
                x: x.clamp(10., SIZE - 10.),
                y: y.clamp(10., SIZE - 10.),
                angle,
            },
            Cell {
                id,
                mode,
                mass: 1.,
                age: 0.,
                generation: 0,
                mutations: 0,
                cooldown: 0.,
            },
            genome,
        ));
        self.births += 1;
        Ok(id)
    }
    pub fn edit(&mut self, id: u32, mode: usize, values: &[f32]) -> Result<(), JsError> {
        let genome = decode(values)?;
        if mode >= genome.0.len() {
            return Err(JsError::new("Invalid mode"));
        }
        for (_, (cell, g)) in self.world.query_mut::<(&mut Cell, &mut Genome)>() {
            if cell.id == id {
                *g = genome;
                cell.mode = mode;
                return Ok(());
            }
        }
        Err(JsError::new("Cell no longer exists"))
    }
    pub fn genome(&self, id: u32) -> Vec<f32> {
        for (_, (c, g)) in &mut self.world.query::<(&Cell, &Genome)>() {
            if c.id == id {
                return g
                    .0
                    .iter()
                    .flat_map(|m| {
                        [
                            m.kind as f32,
                            m.threshold,
                            m.child_a as f32,
                            m.child_b as f32,
                            m.ratio,
                            m.speed,
                            m.angle,
                            m.lifespan,
                            m.can_divide as u8 as f32,
                            m.bind_children as u8 as f32,
                            m.keep_a as u8 as f32,
                            m.keep_b as u8 as f32,
                        ]
                    })
                    .collect();
            }
        }
        Vec::new()
    }
    pub fn cell_mode(&self, id: u32) -> usize {
        self.world
            .query::<&Cell>()
            .iter()
            .find(|(_, c)| c.id == id)
            .map(|(_, c)| c.mode)
            .unwrap_or(0)
    }
    pub fn advance(&mut self, steps: u32) {
        for _ in 0..steps.min(120) {
            self.step();
        }
    }
    pub fn snapshot(&mut self) -> Vec<f32> {
        self.frame.clear();
        let mut rows: Vec<_> = self
            .world
            .query::<(&Position, &Cell, &Genome)>()
            .iter()
            .map(|(_, (p, c, g))| {
                [
                    c.id as f32,
                    p.x,
                    p.y,
                    5. + c.mass.sqrt() * 5.,
                    g.0[c.mode].kind as f32,
                    c.mass,
                    c.age,
                    c.generation as f32,
                    c.mutations as f32,
                    p.angle,
                ]
            })
            .collect();
        rows.sort_by_key(|r| r[0] as u32);
        for row in rows {
            self.frame.extend(row);
        }
        self.frame.clone()
    }
    pub fn terrain(&self) -> Vec<f32> {
        self.tiles
            .iter()
            .flat_map(|t| [t.light, t.radiation, t.food, t.biome])
            .collect()
    }
    pub fn links(&self) -> Vec<u32> {
        self.bonds.iter().flat_map(|&(a, b)| [a, b]).collect()
    }
    pub fn stats(&self) -> Vec<f32> {
        vec![
            self.tick as f32,
            self.world.len() as f32,
            self.births as f32,
            self.deaths as f32,
            self.infections as f32,
        ]
    }
}
impl Simulation {
    fn random(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f64 / u32::MAX as f64) as f32
    }
    fn step(&mut self) {
        self.tick += 1;
        for t in &mut self.tiles {
            t.food = (t.food + STEP * 0.018 * t.capacity).min(t.capacity);
        }
        let mut entities: Vec<(u32, Entity)> = self
            .world
            .query::<&Cell>()
            .iter()
            .map(|(e, c)| (c.id, e))
            .collect();
        entities.sort_by_key(|a| a.0);
        let population = self.world.len() as usize;
        let mut dead = Vec::new();
        let mut children = Vec::new();
        let mut inheritors: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        let mut new_bonds = Vec::new();
        let bonded: HashSet<_> = self.bonds.iter().flat_map(|&(a, b)| [a, b]).collect();
        let mut buckets: BTreeMap<(i32, i32), Vec<(Entity, Position, u8)>> = BTreeMap::new();
        for &(_, e) in &entities {
            let noise = self.random();
            let mutation_roll = self.random();
            let mutation_gene = self.random();
            let (p, c, g) = self
                .world
                .query_one_mut::<(&mut Position, &mut Cell, &mut Genome)>(e)
                .unwrap();
            let m = g.0[c.mode];
            c.age += STEP;
            c.cooldown = (c.cooldown - STEP).max(0.);
            let t = &mut self.tiles[tile_index(p.x, p.y)];
            let gain = if m.kind == 0 {
                t.light * 0.14 * STEP
            } else {
                let eaten = (0.18 * STEP).min(t.food);
                t.food -= eaten;
                eaten * if m.kind == 1 { 1.6 } else { 0.65 }
            };
            c.mass += gain - STEP * (if m.kind == 4 { 0.014 } else { 0.025 } + m.speed * 0.0003);
            if mutation_roll < t.radiation * STEP * 0.035 {
                let index = ((mutation_gene * g.0.len() as f32) as usize).min(g.0.len() - 1);
                let target = &mut g.0[index];
                target.kind = (target.kind + 1) % 6;
                target.threshold = (target.threshold + (noise - 0.5) * 0.8).clamp(1., 8.);
                c.mutations += 1;
            }
            c.mass -= t.radiation * STEP * 0.015;
            // A bonded flagellum supplies directed propulsion to the organism.
            // Passive members have drag, but no independent random locomotion.
            let linked = bonded.contains(&c.id);
            if !linked {
                p.angle += (noise - 0.5) * 0.35;
            }
            let speed = if m.kind == 2 {
                m.speed
            } else if linked {
                0.
            } else {
                m.speed * 0.15
            };
            p.x = (p.x + p.angle.cos() * speed * STEP).clamp(12., SIZE - 12.);
            p.y = (p.y + p.angle.sin() * speed * STEP).clamp(12., SIZE - 12.);
            if c.mass < 0.18 || c.age > m.lifespan {
                dead.push(e);
                continue;
            }
            if m.can_divide && c.mass >= m.threshold && population + children.len() / 2 < LIMIT {
                // parent is replaced by two daughters
                let total = c.mass;
                let mut a = c.clone();
                let mut b = c.clone();
                a.id = self.next_id;
                b.id = self.next_id + 1;
                self.next_id += 2;
                let successors = inheritors.entry(c.id).or_default();
                if m.keep_a {
                    successors.push(a.id);
                }
                if m.keep_b {
                    successors.push(b.id);
                }
                if m.bind_children {
                    new_bonds.push((a.id, b.id));
                }
                a.mass = total * m.ratio;
                b.mass = total * (1. - m.ratio);
                a.mode = m.child_a;
                b.mode = m.child_b;
                a.age = 0.;
                b.age = 0.;
                a.generation += 1;
                b.generation += 1;
                let angle = p.angle + m.angle;
                let distance = (radius(a.mass) + radius(b.mass) + 2.) / 2.;
                let pa = Position {
                    x: (p.x + angle.cos() * distance).clamp(12., SIZE - 12.),
                    y: (p.y + angle.sin() * distance).clamp(12., SIZE - 12.),
                    angle,
                };
                let pb = Position {
                    x: (p.x - angle.cos() * distance).clamp(12., SIZE - 12.),
                    y: (p.y - angle.sin() * distance).clamp(12., SIZE - 12.),
                    angle: angle + std::f32::consts::PI,
                };
                children.push((pa, a, g.clone()));
                children.push((pb, b, g.clone()));
                dead.push(e);
                continue;
            }
            buckets
                .entry(((p.x / 32.) as i32, (p.y / 32.) as i32))
                .or_default()
                .push((e, *p, m.kind));
        }
        let removed: HashSet<_> = dead.iter().copied().collect();
        let mut infections = Vec::new();
        let mut infected = HashSet::new();
        // Stable traversal and local spatial grid bound the interaction search.
        for &(_, e) in &entities {
            if removed.contains(&e) {
                continue;
            }
            let (pos, kind, cooldown) = {
                let (p, c, g) = self
                    .world
                    .query_one_mut::<(&Position, &Cell, &Genome)>(e)
                    .unwrap();
                (*p, g.0[c.mode].kind, c.cooldown)
            };
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if let Some(neighbors) =
                        buckets.get(&((pos.x / 32.) as i32 + dx, (pos.y / 32.) as i32 + dy))
                    {
                        for &(other, op, okind) in neighbors {
                            if e == other || removed.contains(&other) {
                                continue;
                            }
                            let d2 = (pos.x - op.x).powi(2) + (pos.y - op.y).powi(2);
                            if d2 > 24. * 24. {
                                continue;
                            }
                            if kind == 3
                                && okind != 3
                                && !infected.contains(&other)
                                && cooldown <= 0.
                                && self.world.get::<&Cell>(e).unwrap().cooldown <= 0.
                            {
                                let donor = (*self.world.get::<&Genome>(e).unwrap()).clone();
                                infections.push((other, donor));
                                infected.insert(other);
                                self.world.get::<&mut Cell>(e).unwrap().cooldown = 3.;
                                self.infections += 1;
                                break;
                            }
                            if kind == 5 && okind != 4 {
                                let taken = {
                                    let mut c = self.world.get::<&mut Cell>(other).unwrap();
                                    let amount = (0.08 * STEP).min((c.mass - 0.1).max(0.));
                                    c.mass -= amount;
                                    amount
                                };
                                self.world.get::<&mut Cell>(e).unwrap().mass += taken * 0.8;
                            }
                            if d2 > 0.001 && d2 < 18. * 18. {
                                let d = d2.sqrt();
                                let mut p = self.world.get::<&mut Position>(e).unwrap();
                                p.x = (p.x + (pos.x - op.x) / d * 0.3).clamp(12., SIZE - 12.);
                                p.y = (p.y + (pos.y - op.y) / d * 0.3).clamp(12., SIZE - 12.);
                            }
                        }
                    }
                }
            }
        }
        // Commit infections together: a newly infected cell cannot infect again in this tick.
        for (entity, genome) in infections {
            let (cell, target) = self
                .world
                .query_one_mut::<(&mut Cell, &mut Genome)>(entity)
                .unwrap();
            *target = genome;
            // Infection can replace a long genome with a shorter one.
            cell.mode = cell.mode.min(target.0.len() - 1);
            cell.mutations += 1;
            cell.cooldown = 3.;
        }
        let divisions = children.len() / 2;
        self.deaths += (dead.len() - divisions) as u32;
        for e in dead {
            self.world.despawn(e).unwrap();
        }
        for (p, c, g) in children {
            self.world.spawn((p, c, g));
            self.births += 1;
        }
        let living: HashSet<_> = self
            .world
            .query::<&Cell>()
            .iter()
            .map(|(_, c)| c.id)
            .collect();
        let mut bonds = BTreeSet::new();
        // Replace both endpoints together, including neighbors dividing on the same tick.
        for &(a, b) in &self.bonds {
            let old_a = vec![a];
            let old_b = vec![b];
            for &next_a in inheritors.get(&a).unwrap_or(&old_a) {
                for &next_b in inheritors.get(&b).unwrap_or(&old_b) {
                    if next_a != next_b && living.contains(&next_a) && living.contains(&next_b) {
                        bonds.insert((next_a.min(next_b), next_a.max(next_b)));
                    }
                }
            }
        }
        bonds.extend(new_bonds);
        self.bonds = bonds;
        self.solve_bonds();
    }

    fn solve_bonds(&mut self) {
        if self.bonds.is_empty() {
            return;
        }
        let mut bodies: Vec<_> = self
            .world
            .query::<(&Position, &Cell)>()
            .iter()
            .map(|(e, (p, c))| (e, c.id, *p, c.mass))
            .collect();
        bodies.sort_by_key(|b| b.1);
        let indices: BTreeMap<_, _> = bodies.iter().enumerate().map(|(i, b)| (b.1, i)).collect();
        let edges: Vec<_> = self
            .bonds
            .iter()
            .map(|(a, b)| (indices[a], indices[b]))
            .collect();
        // Position constraints distribute propulsion according to mass. Re-evaluate
        // rest length as cells grow; the two cells remain distinct, touching bodies.
        for _ in 0..12 {
            for &(a, b) in &edges {
                let (left, right) = bodies.split_at_mut(b);
                let (_, _, pa, ma) = &mut left[a];
                let (_, _, pb, mb) = &mut right[0];
                let dx = pb.x - pa.x;
                let dy = pb.y - pa.y;
                let distance = dx.hypot(dy);
                let (nx, ny) = if distance > 0.0001 {
                    (dx / distance, dy / distance)
                } else {
                    (1., 0.)
                };
                let error = distance - (radius(*ma) + radius(*mb) + 2.);
                let share_a = *mb / (*ma + *mb);
                let share_b = 1. - share_a;
                pa.x = (pa.x + nx * error * share_a).clamp(12., SIZE - 12.);
                pa.y = (pa.y + ny * error * share_a).clamp(12., SIZE - 12.);
                pb.x = (pb.x - nx * error * share_b).clamp(12., SIZE - 12.);
                pb.y = (pb.y - ny * error * share_b).clamp(12., SIZE - 12.);
            }
        }
        for (entity, _, position, _) in bodies {
            *self.world.get::<&mut Position>(entity).unwrap() = position;
        }
    }
}

fn radius(mass: f32) -> f32 {
    5. + mass.sqrt() * 5.
}
#[cfg(test)]
mod tests {
    use super::*;
    fn modern_genes(count: usize) -> Vec<f32> {
        (0..count)
            .flat_map(|i| {
                [
                    0., 8., i as f32, i as f32, 0.5, 0., 0., 1000., 1., 0., 0., 0.,
                ]
            })
            .collect()
    }
    fn entity(s: &Simulation, id: u32) -> Entity {
        s.world
            .query::<&Cell>()
            .iter()
            .find(|(_, c)| c.id == id)
            .unwrap()
            .0
    }
    fn quiet_world() -> Simulation {
        let mut s = Simulation::new(42);
        for tile in &mut s.tiles {
            tile.radiation = 0.;
        }
        s
    }
    #[test]
    fn all_three_adhesion_flags_are_independent() {
        for bits in 0..8 {
            let mut s = quiet_world();
            let mut g = modern_genes(1);
            g[1] = 1.;
            g[9] = (bits & 1 != 0) as u8 as f32;
            g[10] = (bits & 2 != 0) as u8 as f32;
            g[11] = (bits & 4 != 0) as u8 as f32;
            let parent = s.spawn(500., 500., 0, &g).unwrap();
            let neighbor = s.spawn(522., 500., 0, &modern_genes(1)).unwrap();
            s.bonds.insert((parent, neighbor));
            s.advance(1);
            let mut expected = BTreeSet::new();
            if bits & 1 != 0 {
                expected.insert((3, 4));
            }
            if bits & 2 != 0 {
                expected.insert((neighbor, 3));
            }
            if bits & 4 != 0 {
                expected.insert((neighbor, 4));
            }
            assert_eq!(s.bonds, expected, "flags={bits}");
            assert_eq!(s.world.len(), 3);
        }
    }
    #[test]
    fn adjacent_parents_can_divide_on_the_same_tick() {
        let mut s = quiet_world();
        let mut g = modern_genes(1);
        g[1] = 1.;
        g[10] = 1.;
        let a = s.spawn(500., 500., 0, &g).unwrap();
        let b = s.spawn(522., 500., 0, &g).unwrap();
        s.bonds.insert((a, b));
        s.advance(1);
        assert_eq!(s.links(), vec![3, 5]);
        assert_eq!(s.world.len(), 4);
    }
    #[test]
    fn organism_keeps_flagellum_and_releases_photo_offspring() {
        let mut s = quiet_world();
        let mut g = modern_genes(3);
        g[1] = 1.;
        g[2] = 1.;
        g[3] = 2.;
        g[9] = 1.;
        g[12] = 2.;
        g[12 + 8] = 0.;
        g[24] = 1.;
        g[24 + 1] = 2.4;
        g[24 + 2] = 2.;
        g[24 + 3] = 0.;
        g[24 + 10] = 1.;
        s.spawn(500., 500., 0, &g).unwrap();
        s.advance(1);
        assert_eq!(s.links(), vec![2, 3]);
        let flagellum = entity(&s, 2);
        let phagocyte = entity(&s, 3);
        s.world.get::<&mut Cell>(flagellum).unwrap().mass = 10.;
        s.world.get::<&mut Cell>(phagocyte).unwrap().mass = 3.;
        s.advance(1);
        assert_eq!(s.links(), vec![2, 4]);
        assert_eq!(s.cell_mode(2), 1); // non-dividing flagellum survives above its threshold
        assert_eq!(s.cell_mode(4), 2); // continuing phagocyte inherits the bond
        assert_eq!(s.cell_mode(5), 0); // detached photo offspring
        assert_eq!(s.world.len(), 3);
        assert_eq!(s.deaths, 0);
    }
    #[test]
    fn flagellum_pulls_a_passive_cell_and_bonds_survive_growth() {
        let mut s = quiet_world();
        let mut g = modern_genes(1);
        g[8] = 0.;
        let passive = s.spawn(522., 500., 0, &g).unwrap();
        g[0] = 2.;
        g[5] = 30.;
        let flagellum = s.spawn(500., 500., 0, &g).unwrap();
        let flag_entity = entity(&s, flagellum);
        s.world.get::<&mut Position>(flag_entity).unwrap().angle = 0.;
        s.bonds.insert((passive, flagellum));
        s.advance(30);
        let rows = s.snapshot();
        assert!(rows[1] > 527., "passive cell must be pulled by propulsion");
        let distance = (rows[1] - rows[11]).hypot(rows[2] - rows[12]);
        assert!((distance - rows[3] - rows[13] - 2.).abs() < 0.01);
        assert_eq!(s.links(), vec![passive, flagellum]);
    }
    #[test]
    fn death_and_reset_remove_bonds() {
        let mut s = quiet_world();
        let a = s.spawn(500., 500., 0, &modern_genes(1)).unwrap();
        let b = s.spawn(522., 500., 0, &modern_genes(1)).unwrap();
        s.bonds.insert((a, b));
        let e = entity(&s, a);
        s.world.get::<&mut Cell>(e).unwrap().age = 1001.;
        s.advance(1);
        assert!(s.links().is_empty());
        assert_eq!(s.world.len(), 1);
        let c = s.spawn(544., 500., 0, &modern_genes(1)).unwrap();
        s.bonds.insert((b, c));
        s.reset();
        assert!(s.links().is_empty());
    }
    #[test]
    fn forty_archetypes_and_shorter_infection_are_supported() {
        let mut s = quiet_world();
        let g = modern_genes(40);
        let recipient = s.spawn(500., 500., 39, &g).unwrap();
        assert_eq!(s.genome(recipient).len(), 40 * MODE_STRIDE);
        let mut donor = modern_genes(1);
        donor[0] = 3.;
        s.spawn(505., 500., 0, &donor).unwrap();
        s.advance(2);
        assert_eq!(s.genome(recipient).len(), MODE_STRIDE);
        assert_eq!(s.cell_mode(recipient), 0);
    }
    #[test]
    fn bonded_chain_at_boundary_is_finite_and_deterministic() {
        let run = || {
            let mut s = quiet_world();
            let mut g = modern_genes(1);
            g[8] = 0.;
            g[0] = 2.;
            g[5] = 60.;
            let mut previous = None;
            for i in 0..24 {
                let id = s.spawn(12. + 22. * i as f32, 12., 0, &g).unwrap();
                if let Some(prev) = previous {
                    s.bonds.insert((prev, id));
                }
                previous = Some(id);
            }
            for _ in 0..3 {
                s.advance(120);
            }
            let snapshot = s.snapshot();
            assert!(snapshot.iter().all(|v| v.is_finite()));
            assert_eq!(s.bonds.len(), 23);
            for row in snapshot.chunks_exact(10) {
                assert!((12. ..=SIZE - 12.).contains(&row[1]));
                assert!((12. ..=SIZE - 12.).contains(&row[2]));
            }
            (snapshot, s.links())
        };
        assert_eq!(run(), run());
    }
    fn genes() -> Vec<f32> {
        [0., 2.4, 0., 0., 0.5, 12., 0.8, 240.].repeat(8)
    }
    #[test]
    fn deterministic_replay() {
        let mut a = Simulation::new(42);
        let mut b = Simulation::new(42);
        for s in [&mut a, &mut b] {
            s.spawn(300., 300., 0, &genes()).unwrap();
            s.advance(120);
            s.advance(120);
        }
        assert_eq!(a.snapshot(), b.snapshot());
        assert_eq!(a.terrain(), b.terrain());
    }
    #[test]
    fn division_uses_child_modes_and_conserves_mass() {
        let mut s = Simulation::new(1);
        let mut g = genes();
        g[1] = 1.;
        g[2] = 1.;
        g[3] = 2.;
        g[4] = 0.3;
        s.spawn(100., 100., 0, &g).unwrap();
        s.advance(1);
        let rows = s.snapshot();
        assert_eq!(rows.len(), 20);
        let mass = rows[5] + rows[15];
        assert!((mass - 1.).abs() < 0.01);
        assert!((rows[5] / mass - 0.3).abs() < 0.001);
        let mut modes: Vec<_> = s
            .world
            .query::<&Cell>()
            .iter()
            .map(|(_, c)| c.mode)
            .collect();
        modes.sort();
        assert_eq!(modes, vec![1, 2]);
    }
    #[test]
    fn reset_repeats_seed() {
        let mut s = Simulation::new(9);
        s.spawn(100., 100., 0, &genes()).unwrap();
        let initial = s.snapshot();
        s.advance(100);
        s.reset();
        s.spawn(100., 100., 0, &genes()).unwrap();
        assert_eq!(initial, s.snapshot());
    }
    #[test]
    fn resource_depletion_and_age_death() {
        let mut s = Simulation::new(2);
        let mut g = genes();
        g[0] = 1.;
        g[7] = 20.;
        g[1] = 8.;
        s.spawn(2000., 2500., 0, &g).unwrap();
        let food = s.tiles[tile_index(2000., 2500.)].food;
        s.advance(30);
        assert!(s.tiles[tile_index(2000., 2500.)].food < food);
        for _ in 0..6 {
            s.advance(120);
        }
        assert_eq!(s.world.len(), 0);
        assert_eq!(s.deaths, 1);
    }
    #[test]
    fn infection_copies_genome_and_obeys_cooldown() {
        let mut s = Simulation::new(12);
        let mut donor = genes();
        donor[0] = 3.;
        donor[1] = 7.;
        donor[5] = 0.;
        let mut recipient = genes();
        recipient[1] = 8.;
        recipient[5] = 0.;
        let id = s.spawn(500., 500., 0, &donor).unwrap();
        let other = s.spawn(505., 500., 0, &recipient).unwrap();
        s.spawn(500., 505., 0, &recipient).unwrap();
        s.advance(1);
        assert_eq!(s.infections, 1);
        assert_eq!(s.genome(id), s.genome(other));
        let c = s
            .world
            .query::<&Cell>()
            .iter()
            .find(|(_, c)| c.id == id)
            .unwrap()
            .1
            .clone();
        assert!(c.cooldown > 2.9);
    }
    #[test]
    fn radiation_mutates_genomes() {
        let mut s = Simulation::new(42);
        let mut g = genes();
        g[1] = 8.;
        g[5] = 0.;
        g[7] = 1000.;
        for i in 0..40 {
            s.spawn(
                2300. + (i % 8) as f32 * 25.,
                700. + (i / 8) as f32 * 25.,
                0,
                &g,
            )
            .unwrap();
        }
        for _ in 0..15 {
            s.advance(120);
        }
        assert!(
            s.world
                .query::<&Cell>()
                .iter()
                .any(|(_, c)| c.mutations > 0)
        );
    }
    #[test]
    fn edit_is_isolated_and_boundary_motion_is_finite() {
        let mut s = Simulation::new(17);
        let a = s.spawn(-100., 5000., 0, &genes()).unwrap();
        let b = s.spawn(100., 100., 0, &genes()).unwrap();
        let original = s.genome(b);
        let mut edited = genes();
        edited[0] = 2.;
        edited[5] = 60.;
        s.edit(a, 0, &edited).unwrap();
        assert_eq!(s.genome(b), original);
        s.advance(120);
        for row in s.snapshot().chunks_exact(10) {
            assert!(row.iter().all(|v| v.is_finite()));
            assert!((0. ..=SIZE).contains(&row[1]));
            assert!((0. ..=SIZE).contains(&row[2]));
        }
    }
    #[test]
    fn division_respects_population_limit() {
        let mut s = Simulation::new(8);
        let mut g = genes();
        g[1] = 1.;
        g[5] = 0.;
        for i in 0..LIMIT - 1 {
            s.spawn(
                20. + (i % 80) as f32 * 37.,
                20. + (i / 80) as f32 * 37.,
                0,
                &g,
            )
            .unwrap();
        }
        s.advance(1);
        assert_eq!(s.world.len() as usize, LIMIT);
    }
}
