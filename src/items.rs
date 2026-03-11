use bevy::prelude::*;
use rand::{rngs::StdRng, Rng};
use crate::stats::Stats;
use crate::util::{hex_srgb_u8, weighted_pick};
use itertools::Itertools;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rarity {
    Common,     // grey 30%
    Uncommon,   // green 25%
    Rare,       // blue 20%
    UltraRare,  // violet 14%
    Legendary,  // orange 10%
    Unique,     // gold 1%
}

impl Rarity {
    pub const ALL: [Rarity; 6] = [
        Rarity::Common, Rarity::Uncommon, Rarity::Rare, Rarity::UltraRare, Rarity::Legendary, Rarity::Unique
    ];
    pub fn weights() -> [u32; 6] { [30, 25, 20, 14, 10, 1] }
    pub fn color(self) -> Color {
        match self {
            Rarity::Common    => hex_srgb_u8("#9E9E9E"),
            Rarity::Uncommon  => hex_srgb_u8("#2CB956"),
            Rarity::Rare      => hex_srgb_u8("#4E87FF"),
            Rarity::UltraRare => hex_srgb_u8("#B551FF"),
            Rarity::Legendary => hex_srgb_u8("#FFA036"),
            Rarity::Unique    => hex_srgb_u8("#FFD700"),
        }
    }
}

/// Slots the player can equip
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EquipSlot {
    Watch, Necklace, Gloves, Shirt, Pants, Shoes, Hat, Cape,
    MainHand, OffHand, TwoHanded, Bag,
}

/// Weapon catalogs (examples)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WeaponKind {
    TwoHandedSword, LongSword, DoubleAxe, Scythe, GiantHammer, Book, MagicStaff,
    Dagger, CrystalBall, Hatchet, ShortSword, Lantern,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConsumableKind {
    BigHeal, SmallHeal, BigBandage, SmallBandage,
}

#[derive(Clone, Debug, Default)]
pub struct StatMods {
    pub vigor: i32,
    pub strength: i32,
    pub agility: i32,
    pub magic: i32,
    pub endurance: i32,
}

impl StatMods {
    pub fn add_to(&self, base: &mut Stats) {
        base.vigor     += self.vigor;
        base.strength  += self.strength;
        base.agility   += self.agility;
        base.magic     += self.magic;
        base.endurance += self.endurance;
    }
}

/// Item representation. Some items occupy multiple slots in bag (w,h).
#[derive(Clone, Debug)]
pub struct Item {
    pub name: String,
    pub rarity: Rarity,
    pub equip_slot: Option<EquipSlot>,
    pub weapon: Option<WeaponKind>,
    pub consumable: Option<ConsumableKind>,
    pub size: (u8, u8),   // width x height in the bag grid
    pub mods: StatMods,   // only applied when equipped
    pub extra_rolls: Vec<(String, i32)>, // explain random attributes rolled
}

impl Item {
    pub fn is_bag_upgrade(&self) -> bool {
        self.equip_slot == Some(EquipSlot::Bag)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BagSize { pub w: u8, pub h: u8 }

pub fn bag_size_for_rarity(r: Rarity) -> BagSize {
    match r {
        // common 5x14, uncommon 6x20, rare 8x22, ultra 10x20, legendary 24x24, unique 100x100
        Rarity::Common    => BagSize { w: 5,  h: 14 },
        Rarity::Uncommon  => BagSize { w: 6,  h: 20 },
        Rarity::Rare      => BagSize { w: 8,  h: 22 },
        Rarity::UltraRare => BagSize { w: 10, h: 20 },
        Rarity::Legendary => BagSize { w: 24, h: 24 },
        Rarity::Unique    => BagSize { w: 100,h: 100 },
    }
}

/// Roll an equipment item category randomly (weighted evenly among categories)
pub fn roll_equip_category(rng: &mut StdRng) -> EquipSlot {
    use EquipSlot::*;
    let cats = [Watch, Necklace, Gloves, Shirt, Pants, Shoes, Hat, Cape, MainHand, OffHand, TwoHanded, Bag];
    cats[rng.gen_range(0..cats.len())]
}

/// Your rarity‑driven stat roll rules.
/// For Unique: +random (10..=500) to a single attribute.
pub fn roll_item(rng: &mut StdRng) -> Item {
    let rix = weighted_pick(rng, &Rarity::weights());
    let rarity = Rarity::ALL[rix];

    // Decide if it’s a consumable or an equippable (most rolls equippable)
    let is_consumable = rng.gen_bool(0.15);

    if is_consumable {
        // Consumables occupy 1x1 or 1x2 slots and don’t grant stats directly.
        use ConsumableKind::*;
        let kinds = [BigHeal, SmallHeal, BigBandage, SmallBandage];
        let kind = kinds[rng.gen_range(0..kinds.len())];
        let (name, size) = match kind {
            BigHeal      => ("Big Heal Potion",  (1,2)), // heals full over 30s
            SmallHeal    => ("Small Heal Potion",(1,1)), // heals 25% over 20s
            BigBandage   => ("Big Bandage",      (1,2)), // 50% over 6s
            SmallBandage => ("Small Bandage",    (1,1)), // 25% over 4s
        };
        return Item {
            name: name.into(),
            rarity,
            equip_slot: None,
            weapon: None,
            consumable: Some(kind),
            size: (size.0, size.1),
            mods: StatMods::default(),
            extra_rolls: vec![],
        };
    }

    // Equipment
    let slot = roll_equip_category(rng);
    
    // -- inside roll_item() --

    let mut mods = StatMods::default();
    let mut extra = vec![];

    // add a delta to a given key
    let mut add_key = |key: i32, delta: i32| {
        match key {
            0 => { mods.vigor += delta;     extra.push(("vigor".into(), delta)); }
            1 => { mods.strength += delta;  extra.push(("strength".into(), delta)); }
            2 => { mods.agility += delta;   extra.push(("agility".into(), delta)); }
            3 => { mods.magic += delta;     extra.push(("magic".into(), delta)); }
            _ => { mods.endurance += delta; extra.push(("endurance".into(), delta)); }
        }
    };

    // pick a random key and add delta
    let mut add_rand_delta = |rng: &mut StdRng, delta: i32| {
        let key = rng.gen_range(0..5);
        add_key(key, delta);
    };

    // primary bump
    let mut add_primary = |rng: &mut StdRng, delta: i32| {
        add_rand_delta(rng, delta);
    };

    match rarity {
        Rarity::Common => {
            add_primary(rng, 2);
        }
        Rarity::Uncommon => {
            add_primary(rng, 3);
            let d = rng.gen_range(-10..=10);
            add_rand_delta(rng, d);
        }
        Rarity::Rare => {
            add_primary(rng, 4);
            for _ in 0..2 { 
                let d = rng.gen_range(-10..=10);
                add_rand_delta(rng, d); 
            }
        }
        Rarity::UltraRare => {
            add_primary(rng, 5);
            for _ in 0..3 { 
                let d = rng.gen_range(-10..=10);
                add_rand_delta(rng, d); 
            }
        }
        Rarity::Legendary => {
            add_primary(rng, 7);
            add_primary(rng, 7);
            for _ in 0..3 {
                let d = rng.gen_range(-10..=10); 
                add_rand_delta(rng, d); 
            }
        }
        Rarity::Unique => {
            let d = rng.gen_range(10..=500);
            add_primary(rng, d);
        }
    }

    // Weapons & bags special-cased
    let (weapon, size, name, equip_slot) = match slot {
        EquipSlot::TwoHanded => {
            use WeaponKind::*;
            let w = [TwoHandedSword, LongSword, DoubleAxe, Scythe, GiantHammer, Book, MagicStaff];
            let wk = w[rng.gen_range(0..w.len())];
            (Some(wk), (2,4), format!("{wk:?}"), Some(EquipSlot::TwoHanded))
        }
        EquipSlot::MainHand | EquipSlot::OffHand => {
            use WeaponKind::*;
            let w = [Dagger, CrystalBall, Hatchet, ShortSword, Lantern];
            let wk = w[rng.gen_range(0..w.len())];
            (Some(wk), (2,2), format!("{wk:?}"), Some(slot))
        }
        EquipSlot::Bag => {
            // bag upgrades change inventory capacity
            let bs = super::items::bag_size_for_rarity(rarity);
            (None, (2,3), format!("{:?} Bag {}x{}", rarity, bs.w, bs.h), Some(EquipSlot::Bag))
        }
        _ => (None, (2,2), format!("{:?}", slot), Some(slot)),
    };

    Item {
        name,
        rarity,
        equip_slot,
        weapon,
        consumable: None,
        size,
        mods,
        extra_rolls: extra,
    }
}