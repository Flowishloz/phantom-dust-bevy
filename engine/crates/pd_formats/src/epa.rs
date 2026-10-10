//! `Data/com/param/enemyparam.epa`: the CPU difficulty rows (P0-AI.md section 2.5).
//!
//! Layout (READ from the shipped file, 1,004 bytes, and the readers `FUN_14017b020` /
//! `FUN_14017b100`): `u32` row count (82 in the shipped file) then 10-byte rows. The file
//! carries 100 rows; rows 82..99 are zero and unused. Byte 0 of a row is the starting Health
//! (20 in every used row, used when the CPU seat is built through `FUN_14024e790`); bytes 1..9
//! are the nine signed AI parameters, copied (`(int)*(char*)`, sign extended) into script
//! registers v30..v38 (`ai + 0x20EC .. 0x210C`, register k = `ai + 0x2074 + 4k`).

use crate::bytes::{slice, u32_at};
use anyhow::{bail, Result};

pub const ROW_SIZE: usize = 10;
/// First script register that receives a parameter (v30).
pub const FIRST_REGISTER: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EnemyParamRow {
    pub health: u8,
    /// Parameters 0..8 = registers v30..v38.
    pub params: [i8; 9],
}

impl EnemyParamRow {
    /// v30 target rule: 0 lowest slot, 1 highest slot, 2 lowest Health, 3 highest Health,
    /// 4 nearest, 5 farthest, 6 random (script 0x03AC).
    pub fn target_rule(&self) -> i8 {
        self.params[0]
    }
    /// v31 think interval in ticks (30 in every row).
    pub fn think_interval(&self) -> i8 {
        self.params[1]
    }
    /// v32 attack probability %.
    pub fn attack_pct(&self) -> i8 {
        self.params[2]
    }
    /// v33 react to a bullet %.
    pub fn react_bullet_pct(&self) -> i8 {
        self.params[3]
    }
    /// v34 react to melee %.
    pub fn react_melee_pct(&self) -> i8 {
        self.params[4]
    }
    /// v35 dodge %.
    pub fn dodge_pct(&self) -> i8 {
        self.params[5]
    }
    /// v36 wander %.
    pub fn wander_pct(&self) -> i8 {
        self.params[6]
    }
    /// v37 idle style: 0 avoid, 1 roam, 2 camp the target's start point.
    pub fn idle_style(&self) -> i8 {
        self.params[7]
    }
    /// v38 capsule-hunt strategy 0..3.
    pub fn capsule_hunt(&self) -> i8 {
        self.params[8]
    }
    /// `(register, value)` pairs the program writes at creation, v30..v38.
    pub fn registers(&self) -> [(usize, i64); 9] {
        std::array::from_fn(|i| (FIRST_REGISTER + i, self.params[i] as i64))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnemyParams {
    /// The header count (82 in the shipped file).
    pub declared: u32,
    pub rows: Vec<EnemyParamRow>,
}

impl EnemyParams {
    pub fn parse(data: &[u8]) -> Result<EnemyParams> {
        let declared = u32_at(data, 0)?;
        if data.len() < 4 || (data.len() - 4) % ROW_SIZE != 0 {
            bail!("epa: {} bytes is not 4 + n * {ROW_SIZE}", data.len());
        }
        let n = (data.len() - 4) / ROW_SIZE;
        if (declared as usize) > n {
            bail!("epa: header says {declared} rows but the file holds {n}");
        }
        let mut rows = Vec::with_capacity(n);
        for i in 0..n {
            let r = slice(data, 4 + i * ROW_SIZE, ROW_SIZE)?;
            rows.push(EnemyParamRow { health: r[0], params: std::array::from_fn(|k| r[1 + k] as i8) });
        }
        Ok(EnemyParams { declared, rows })
    }

    pub fn row(&self, id: usize) -> Option<&EnemyParamRow> {
        self.rows.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rows_and_registers() {
        // row 0 of the shipped file: health 20, rule 0, think 30, 88 88 88 88, wander 3, idle 0, hunt 0
        let mut d = vec![2, 0, 0, 0];
        d.extend_from_slice(&[0x14, 0x00, 0x1E, 0x58, 0x58, 0x58, 0x58, 0x03, 0x00, 0x00]);
        d.extend_from_slice(&[0x14, 0x01, 0x1E, 0x4D, 0x4D, 0x4D, 0x4D, 0x06, 0xFF, 0x03]);
        let e = EnemyParams::parse(&d).unwrap();
        assert_eq!(e.declared, 2);
        let r = e.row(0).unwrap();
        assert_eq!((r.health, r.target_rule(), r.think_interval(), r.attack_pct(), r.wander_pct()), (20, 0, 30, 88, 3));
        assert_eq!(r.registers()[0], (30, 0));
        assert_eq!(r.registers()[8], (38, 0));
        // sign extension like `(int)*(char*)` in FUN_14017b020
        assert_eq!(e.row(1).unwrap().registers()[7], (37, -1));
        assert!(e.row(2).is_none());
        assert!(EnemyParams::parse(&d[..13]).is_err());
    }

    #[test]
    fn shipped_file_matches_the_audit() {
        let p = std::env::var_os("PD_GAME_FILES_COM").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(r"..\..\..\game_files\Assets\Data\com"));
        let Ok(d) = std::fs::read(p.join("param/enemyparam.epa")) else {
            eprintln!("no game_files: epa test skipped");
            return;
        };
        let e = EnemyParams::parse(&d).unwrap();
        assert_eq!(e.declared, 82);
        assert_eq!(e.rows.len(), 100);
        // P0-AI 2.5: Health 20 and think interval 30 in every used row; rows 82..99 are zero
        for (i, r) in e.rows.iter().enumerate().take(82) {
            assert_eq!(r.health, 20, "row {i}");
            assert_eq!(r.think_interval(), 30, "row {i}");
        }
        assert!(e.rows[82..].iter().all(|r| *r == EnemyParamRow::default()));
        // rows 0-4: rule 0, attack ladder 88/77/66/55/44 with wander 3/6/9/12/15
        let ladder: Vec<(i8, i8)> = (0..5).map(|i| (e.rows[i].attack_pct(), e.rows[i].wander_pct())).collect();
        assert_eq!(ladder, vec![(88, 3), (77, 6), (66, 9), (55, 12), (44, 15)]);
        // row 70 never attacks, 73/74 wander 80 %
        assert_eq!(e.rows[70].attack_pct(), 0);
        assert_eq!((e.rows[73].wander_pct(), e.rows[74].wander_pct()), (80, 80));
    }
}
