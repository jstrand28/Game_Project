use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use crate::items::{Item, EquipSlot, BagSize, bag_size_for_rarity};
use crate::stats::Stats;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChestTier {
    Common,
    Rare,
    Epic,
    Royal,
}

#[derive(Component)]
pub struct Chest {
    pub items: Vec<Item>,
    pub gold: i32,
    pub tier: ChestTier,
}

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct BagGrid {
    pub w: u8,
    pub h: u8,
    pub cells: Vec<Option<Item>>, // row-major
}

impl BagGrid {
    pub fn new(w: u8, h: u8) -> Self {
        Self { w, h, cells: vec![None; (w as usize)*(h as usize)] }
    }
    fn idx(&self, x: u8, y: u8) -> usize { y as usize * self.w as usize + x as usize }

    pub fn try_add(&mut self, item: Item) -> bool {
        // naive placement: find first free cell and ensure item size fits
        for y in 0..self.h {
            for x in 0..self.w {
                if self.fits_at(x, y, item.size) {
                    self.place(x, y, item);
                    return true;
                }
            }
        }
        false
    }
    fn fits_at(&self, x: u8, y: u8, size: (u8,u8)) -> bool {
        let (w,h) = size;
        if x + w > self.w || y + h > self.h { return false; }
        for yy in y..y+h { for xx in x..x+w {
            if self.cells[self.idx(xx, yy)].is_some() { return false; }
        }}
        true
    }
    pub fn place(&mut self, x: u8, y: u8, item: Item) {
        // mark only top-left with item; for simplicity we store item in top-left
        // (enhance later to track spans)
        let i = self.idx(x,y);
        self.cells[i] = Some(item);
    }
    pub fn remove_at(&mut self, x: u8, y: u8) -> Option<Item> {
        let i = self.idx(x,y);
        self.cells[i].take()

    }
    pub fn resize_to(&mut self, new: BagSize) {
        let mut n = BagGrid::new(new.w, new.h);
        // move over any existing items that still fit
        for y in 0..self.h { 
            for x in 0..self.w {
                let i = self.idx(x,y);
                if let Some(it) = self.cells[i].take() {
                    n.try_add(it);
                }
            }
        }
        *self = n;
    }
}

#[derive(Resource, Default, Clone, Serialize, Deserialize)]
pub struct Equipment {
    pub watch: Option<Item>,
    pub necklace: Option<Item>,
    pub gloves: Option<Item>,
    pub shirt: Option<Item>,
    pub pants: Option<Item>,
    pub shoes: Option<Item>,
    pub hat: Option<Item>,
    pub cape: Option<Item>,
    pub mainhand: Option<Item>,
    pub offhand: Option<Item>,
    pub twohand: Option<Item>,
    pub bag: Option<Item>, // governs grid size
}

impl Equipment {
    pub fn equip(&mut self, item: Item, bag: &mut BagGrid) -> Option<Item> {
        use EquipSlot::*;
        match item.equip_slot {
            Some(Watch) => {
                let mut new = Some(item);
                std::mem::swap(&mut self.watch, &mut new);
                new
            }
            Some(Necklace) => {
                let mut new = Some(item);
                std::mem::swap(&mut self.necklace, &mut new);
                new
            }
            Some(Gloves) => { let mut new = Some(item); std::mem::swap(&mut self.gloves, &mut new); new }
            Some(Shirt)  => { let mut new = Some(item); std::mem::swap(&mut self.shirt,  &mut new); new }
            Some(Pants)  => { let mut new = Some(item); std::mem::swap(&mut self.pants,  &mut new); new }
            Some(Shoes)  => { let mut new = Some(item); std::mem::swap(&mut self.shoes,  &mut new); new }
            Some(Hat)    => { let mut new = Some(item); std::mem::swap(&mut self.hat,    &mut new); new }
            Some(Cape)   => { let mut new = Some(item); std::mem::swap(&mut self.cape,   &mut new); new }

            Some(MainHand) => {
                // if two-handed is equipped, moving to main-hand should free it
                if let Some(old_two) = self.twohand.take() {
                    // give the displaced two-handed item back to caller if no other swap
                    let mut new = Some(item);
                    std::mem::swap(&mut self.mainhand, &mut new);
                    // prefer returning the item we just replaced; stash `old_two` if nothing was there
                    if new.is_none() { new = Some(old_two); }
                    new
                } else {
                    let mut new = Some(item);
                    std::mem::swap(&mut self.mainhand, &mut new);
                    new
                }
            }
            Some(OffHand) => {
                if let Some(old_two) = self.twohand.take() {
                    let mut new = Some(item);
                    std::mem::swap(&mut self.offhand, &mut new);
                    if new.is_none() { new = Some(old_two); }
                    new
                } else {
                    let mut new = Some(item);
                    std::mem::swap(&mut self.offhand, &mut new);
                    new
                }
            }
            Some(TwoHanded) => {
                // two-handed displaces both hands
                let mut new = Some(item);
                let displaced_main = self.mainhand.take();
                let displaced_off  = self.offhand.take();
                std::mem::swap(&mut self.twohand, &mut new);
                // If nothing was already in twohand, return something we displaced from hands
                if new.is_none() { new = displaced_main.or(displaced_off); }
                new
            }
            Some(Bag) => {
                // bag upgrade: resize before swapping
                let sz = bag_size_for_rarity(item.rarity);
                bag.resize_to(sz);
                let mut new = Some(item);
                std::mem::swap(&mut self.bag, &mut new);
                new
            }
            None => Some(item), // non-equipable: just return it to caller
        }
    }

    pub fn sum_mods_into(&self, base: &mut Stats) {
        for it in [ &self.watch, &self.necklace, &self.gloves, &self.shirt, &self.pants,
                    &self.shoes, &self.hat, &self.cape, &self.mainhand, &self.offhand,
                    &self.twohand, &self.bag ] {
            if let Some(i) = it {
                i.mods.add_to(base);
            }
        }
    }
}