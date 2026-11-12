//! Поводок (`Leashable.LeashData`): к кому привязана сущность и NBT.
//!
//! В `leash` лежит либо `UUID` держателя-сущности (игрок/моб/лодка), либо
//! `X/Y/Z` блока забора (`LeashFenceKnotEntity`, узел на заборе).

use chunkwake_world::nbt::{Compound, Tag};
use uuid::Uuid;

/// `Leashable.LEASH_TOO_FAR_DIST`: дальше — поводок рвётся.
pub const TOO_FAR: f64 = 12.0;
/// `Leashable.LEASH_ELASTIC_DIST`: с этой дистанции тянет.
pub const ELASTIC: f64 = 6.0;
/// Сколько тиков держать привязь к пропавшему держателю
/// (`Leashable.restoreLeashFromSave`: после 100 — предмет в дроп).
pub const LOST_TICKS: u32 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeashHolder {
    /// Держатель — сущность (игрок, моб или лодка) по UUID.
    Entity(Uuid),
    /// Узел на заборе (`LeashFenceKnotEntity`).
    Knot([i32; 3]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leash {
    pub holder: LeashHolder,
    /// Тики без держателя; после [`LOST_TICKS`] поводок падает.
    pub lost: u32,
}

impl Leash {
    pub fn to(holder: LeashHolder) -> Self {
        Self { holder, lost: 0 }
    }
}

/// `leash`: `{UUID:[4 int]}` для сущности или `{X,Y,Z}` для узла.
pub fn to_nbt(leash: &Leash) -> Tag {
    let mut c = Compound::new();
    match leash.holder {
        LeashHolder::Entity(uuid) => {
            let u = uuid.as_u128();
            c.insert(
                "UUID".into(),
                Tag::IntArray(
                    (0..4)
                        .rev()
                        .map(|i| (u >> (32 * i)) as u32 as i32)
                        .collect(),
                ),
            );
        }
        LeashHolder::Knot([x, y, z]) => {
            c.insert("X".into(), Tag::Int(x));
            c.insert("Y".into(), Tag::Int(y));
            c.insert("Z".into(), Tag::Int(z));
        }
    }
    Tag::Compound(c)
}

pub fn from_nbt(c: &Compound) -> Option<Leash> {
    if let Some(Tag::IntArray(u)) = c.get("UUID")
        && u.len() == 4
    {
        let u = u.iter().fold(0u128, |acc, &w| acc << 32 | w as u32 as u128);
        return Some(Leash::to(LeashHolder::Entity(Uuid::from_u128(u))));
    }
    let x = c.get("X").and_then(Tag::as_i64)?;
    let y = c.get("Y").and_then(Tag::as_i64)?;
    let z = c.get("Z").and_then(Tag::as_i64)?;
    Some(Leash::to(LeashHolder::Knot([x as i32, y as i32, z as i32])))
}

/// Центр коробки сущности по её ногам и высоте (`leashDistanceTo`).
pub fn center(pos: [f64; 3], height: f64) -> [f64; 3] {
    [pos[0], pos[1] + height / 2.0, pos[2]]
}

/// Импульс натяжения: направление к держателю, сила 0.05 по горизонтали и
/// 0.2 от неё по вертикали (`AXIS_SPECIFIC_ELASTICITY`).
pub fn pull(from: [f64; 3], to: [f64; 3]) -> Option<[f64; 3]> {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len < 1e-6 {
        return None;
    }
    let k = 0.05 / len;
    Some([d[0] * k, d[1] * k * 0.2, d[2] * k])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_leash_nbt_round_trips() {
        let uuid = Uuid::from_u128(0x1122_3344_5566_7788_99aa_bbcc_ddee_ff00);
        let tag = to_nbt(&Leash::to(LeashHolder::Entity(uuid)));
        let Tag::Compound(c) = tag else {
            panic!("не compound");
        };
        assert_eq!(from_nbt(&c), Some(Leash::to(LeashHolder::Entity(uuid))));
    }

    #[test]
    fn knot_leash_nbt_round_trips() {
        let leash = Leash::to(LeashHolder::Knot([10, 64, -3]));
        let Tag::Compound(c) = to_nbt(&leash) else {
            panic!("не compound");
        };
        assert_eq!(from_nbt(&c), Some(leash));
    }
}
