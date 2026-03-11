use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub vigor: i32,     // health & regen speed
    pub strength: i32,  // dmg multiplier & reduction
    pub agility: i32,   // dexterity & interaction speed
    pub magic: i32,     // number of spells & spell power
    pub endurance: i32, // magic capacity & stamina
}

impl Stats {
    pub fn base() -> Self {
        // base is 5 in each category
        Self { vigor: 5, strength: 5, agility: 5, magic: 5, endurance: 5 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BodyPart { Head, Chest, Stomach, LeftArm, RightArm, LeftLeg, RightLeg }

#[derive(Clone, Debug)]
pub struct LimbHealth {
    pub max_hp: f32,
    pub hp: f32,
}

#[derive(Resource, Clone, Debug)]
pub struct PlayerStats {
    pub base: Stats,
    pub bonus: Stats,            // sum of equipped items
    pub limbs: HashMap<BodyPart, LimbHealth>,
}

impl Default for PlayerStats {
    fn default() -> Self {
        let mut s = Self {
            base: Stats::base(),
            bonus: Stats::default(),
            limbs: HashMap::new(),
        };
        s.recompute_limbs();
        s
    }
}

impl PlayerStats {
    /// Your scaling rule (summarized):
    /// - Base is 5 points per stat, treated as "1/3 of 100%".
    /// - 1–15 points => 100% baseline. 16–25 => another 100% (so 200% at 25).
    /// - 40 points ~ base * 5/3 multiplier example.
    /// We implement a continuous stepped multiplier for limb HP & regen.
    pub fn scalar_for(&self, val: i32) -> f32 {
        // Treat first 15 as 1.0, next 10 as +1.0, remainder scaled.
        if val <= 15 { 1.0 }
        else if val <= 25 { 2.0 }
        else {
            // For values above 25, every +10 adds +1.0 (approx). Smooth:
            2.0 + ((val - 25) as f32) / 10.0
        }
    }

    fn total(&self) -> Stats {
        Stats {
            vigor: self.base.vigor + self.bonus.vigor,
            strength: self.base.strength + self.bonus.strength,
            agility: self.base.agility + self.bonus.agility,
            magic: self.base.magic + self.bonus.magic,
            endurance: self.base.endurance + self.bonus.endurance,
        }
    }

    pub fn recompute_limbs(&mut self) {
        let t = self.total();

        // Base HPs:
        // head 50hp, chest & stomach 85hp, arms/legs 60hp
        // Then scale by vigor scalar.
        let scale = self.scalar_for(t.vigor);
        let mut set = |p: BodyPart, base_hp: f32| {
            let max_hp = base_hp * scale;
            Self::upsert(&mut self.limbs, p, max_hp);
        };
        set(BodyPart::Head,    50.0);
        set(BodyPart::Chest,   85.0);
        set(BodyPart::Stomach, 85.0);
        set(BodyPart::LeftArm, 60.0);
        set(BodyPart::RightArm,60.0);
        set(BodyPart::LeftLeg, 60.0);
        set(BodyPart::RightLeg,60.0);
    }

    fn upsert(map: &mut HashMap<BodyPart, LimbHealth>, p: BodyPart, max_hp: f32) {
        map.entry(p).and_modify(|lh| { lh.max_hp = max_hp; lh.hp = lh.hp.min(max_hp); })
                     .or_insert(LimbHealth { max_hp, hp: max_hp });
    }
}